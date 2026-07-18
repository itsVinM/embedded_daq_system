use shared::{FaultConfig, FaultType, FaultResult, FaultInjector};
use super::{bit_flip, bit_set, bit_clear, busy_delay_us, Lfsr, should_inject};

pub struct CanBus {
    pub id: u32,
    pub data: [u8; 8],
    pub dlc: u8,
}

pub struct CanFaultInjector<'d> {
    config: FaultConfig,
    lfsr: Lfsr,
    armed: bool,
    count: u32,
    _lifetime: core::marker::PhantomData<&'d ()>,
}

impl<'d> CanFaultInjector<'d> {
    pub fn new() -> Self {
        Self {
            config: FaultConfig::new(shared::Protocol::Can, FaultType::BitFlip),
            lfsr: Lfsr::new(0x1234),
            armed: false,
            count: 0,
            _lifetime: core::marker::PhantomData,
        }
    }

    pub fn inject_id(&mut self, id: u32) -> u32 {
        if !self.armed || !should_inject(&mut self.lfsr, self.config.probability_permille) {
            return id;
        }
        let bit = if self.config.target_bit < 29 {
            self.config.target_bit
        } else {
            self.lfsr.next_bit() % 29
        };
        let result = match self.config.fault_type {
            FaultType::BitFlip => id ^ (1 << bit),
            FaultType::StuckAtZero => id & !(1 << bit),
            FaultType::StuckAtOne => id | (1 << bit),
            _ => id,
        };
        if result != id { self.count += 1; }
        result
    }

    pub fn inject_data(&mut self, byte: u8, index: usize) -> u8 {
        if !self.armed || !should_inject(&mut self.lfsr, self.config.probability_permille) {
            return byte;
        }
        let result = match self.config.fault_type {
            FaultType::BitFlip => {
                let bit = if self.config.target_bit < 8 { self.config.target_bit } else { self.lfsr.next_bit() };
                bit_flip(byte, bit)
            }
            FaultType::CrcCorrupt if index >= 4 => {
                bit_flip(byte, self.lfsr.next_bit())
            }
            FaultType::FrameCorrupt => {
                bit_flip(byte, self.lfsr.next_bit())
            }
            _ => byte,
        };
        if result != byte { self.count += 1; }
        result
    }

    pub fn inject_stuff_error(&self) -> bool {
        self.armed && self.config.fault_type == FaultType::ClockGlitch
            && should_inject(&mut Lfsr::new(0), self.config.probability_permille)
    }

    pub fn inject_form_error(&self) -> bool {
        self.armed && self.config.fault_type == FaultType::FrameCorrupt
            && should_inject(&mut Lfsr::new(0), self.config.probability_permille)
    }
}

impl<'d, 'b> FaultInjector<'d, CanBus> for CanFaultInjector<'d> {
    type Error = ();

    fn configure(&mut self, config: &FaultConfig) -> Result<(), ()> {
        self.config = *config;
        Ok(())
    }

    fn arm(&mut self) -> Result<(), ()> {
        self.armed = true;
        self.count = 0;
        Ok(())
    }

    fn disarm(&mut self) -> Result<(), ()> {
        self.armed = false;
        Ok(())
    }

    fn fire(&mut self, bus: &mut CanBus) -> Result<FaultResult, ()> {
        if !self.armed { return Err(()); }
        bus.id = self.inject_id(bus.id);
        for (i, byte) in bus.data.iter_mut().enumerate() {
            *byte = self.inject_data(*byte, i);
        }
        Ok(FaultResult::Fired)
    }

    fn is_armed(&self) -> bool { self.armed }
    fn injected_count(&self) -> u32 { self.count }
    fn reset_stats(&mut self) { self.count = 0; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_bit_flip_id() {
        let cfg = FaultConfig::new(shared::Protocol::Can, FaultType::BitFlip).at_bit(0);
        let mut inj = CanFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert_eq!(inj.inject_id(0x0000_0000), 0x0000_0001);
    }

    #[test]
    fn can_stuck_at_zero() {
        let cfg = FaultConfig::new(shared::Protocol::Can, FaultType::StuckAtZero).at_bit(3);
        let mut inj = CanFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert_eq!(inj.inject_id(0xFFFF_FFFF), 0xFFFF_FFF7);
    }

    #[test]
    fn can_crc_corrupt() {
        let cfg = FaultConfig::new(shared::Protocol::Can, FaultType::CrcCorrupt);
        let mut inj = CanFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        let d0 = inj.inject_data(0xFF, 0);
        let d4 = inj.inject_data(0xFF, 4);
        assert_eq!(d0, 0xFF);
        assert_ne!(d4, 0xFF);
    }

    #[test]
    fn can_trait_fire() {
        let cfg = FaultConfig::new(shared::Protocol::Can, FaultType::BitFlip).at_bit(0);
        let mut inj = CanFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        let mut bus = CanBus { id: 0, data: [0; 8], dlc: 8 };
        assert_eq!(inj.fire(&mut bus).unwrap(), FaultResult::Fired);
        assert_eq!(bus.id, 0x0000_0001);
    }
}
