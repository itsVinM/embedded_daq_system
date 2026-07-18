use shared::{FaultConfig, FaultType, FaultResult, FaultInjector};
use super::{bit_flip, bit_set, bit_clear, busy_delay_us, Lfsr, should_inject};

pub struct OneWireBus {
    pub line: bool,
}

pub struct OneWireFaultInjector<'d> {
    config: FaultConfig,
    lfsr: Lfsr,
    armed: bool,
    count: u32,
    _lifetime: core::marker::PhantomData<&'d ()>,
}

impl<'d> OneWireFaultInjector<'d> {
    pub fn new() -> Self {
        Self {
            config: FaultConfig::new(shared::Protocol::OneWire, FaultType::BitFlip),
            lfsr: Lfsr::new(0x5678),
            armed: false,
            count: 0,
            _lifetime: core::marker::PhantomData,
        }
    }

    pub fn suppress_presence(&self) -> bool {
        self.armed && self.config.fault_type == FaultType::Timeout
            && should_inject(&mut Lfsr::new(0), self.config.probability_permille)
    }

    pub fn inject_data(&mut self, byte: u8, index: usize) -> u8 {
        if !self.armed || !should_inject(&mut self.lfsr, self.config.probability_permille) {
            return byte;
        }
        let bit = if self.config.target_bit < 8 {
            self.config.target_bit
        } else {
            self.lfsr.next_bit()
        };
        let result = match self.config.fault_type {
            FaultType::BitFlip => bit_flip(byte, bit),
            FaultType::StuckAtZero => bit_clear(byte, bit),
            FaultType::StuckAtOne => bit_set(byte, bit),
            FaultType::CrcCorrupt if index >= 8 => bit_flip(byte, self.lfsr.next_bit()),
            _ => byte,
        };
        if result != byte { self.count += 1; }
        result
    }

    pub fn inject_rom_command(&mut self, cmd: u8) -> u8 {
        if !self.armed || !should_inject(&mut self.lfsr, self.config.probability_permille) {
            return cmd;
        }
        match self.config.fault_type {
            FaultType::BitFlip => {
                let bit = if self.config.target_bit < 8 {
                    self.config.target_bit
                } else {
                    self.lfsr.next_bit()
                };
                let r = bit_flip(cmd, bit);
                if r != cmd { self.count += 1; }
                r
            }
            _ => cmd,
        }
    }

    pub fn inject_timing_violation(&self) {
        if self.armed && self.config.fault_type == FaultType::BitDelay
            && should_inject(&mut Lfsr::new(0), self.config.probability_permille)
        {
            busy_delay_us(self.config.duration_us);
        }
    }

    pub fn glitch_reset_pulse(&self) -> bool {
        self.armed && self.config.fault_type == FaultType::ClockGlitch
            && should_inject(&mut Lfsr::new(0), self.config.probability_permille)
    }
}

impl<'d, 'b> FaultInjector<'d, OneWireBus> for OneWireFaultInjector<'d> {
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

    fn fire(&mut self, bus: &mut OneWireBus) -> Result<FaultResult, ()> {
        if !self.armed { return Err(()); }
        if self.config.fault_type == FaultType::Timeout && self.suppress_presence() {
            busy_delay_us(self.config.duration_us);
        }
        if self.config.fault_type == FaultType::ClockGlitch {
            self.glitch_reset_pulse();
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
    fn onewire_bit_flip() {
        let cfg = FaultConfig::new(shared::Protocol::OneWire, FaultType::BitFlip).at_bit(0);
        let mut inj = OneWireFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert_eq!(inj.inject_data(0b1111_1110, 0), 0b1111_1111);
    }

    #[test]
    fn onewire_crc_corrupt_after_index8() {
        let cfg = FaultConfig::new(shared::Protocol::OneWire, FaultType::CrcCorrupt);
        let mut inj = OneWireFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert_eq!(inj.inject_data(0xFF, 0), 0xFF);
        assert_ne!(inj.inject_data(0xFF, 10), 0xFF);
    }

    #[test]
    fn onewire_presence_suppressed() {
        let cfg = FaultConfig::new(shared::Protocol::OneWire, FaultType::Timeout);
        let mut inj = OneWireFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert!(inj.suppress_presence());
    }

    #[test]
    fn onewire_rom_cmd_flip() {
        let cfg = FaultConfig::new(shared::Protocol::OneWire, FaultType::BitFlip).at_bit(0);
        let mut inj = OneWireFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert_eq!(inj.inject_rom_command(0xCC), 0xCD);
    }

    #[test]
    fn onewire_trait_fire() {
        let cfg = FaultConfig::new(shared::Protocol::OneWire, FaultType::BitFlip);
        let mut inj = OneWireFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        let mut bus = OneWireBus { line: true };
        assert_eq!(inj.fire(&mut bus).unwrap(), FaultResult::Fired);
    }
}
