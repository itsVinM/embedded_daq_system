use shared::{FaultConfig, FaultType, FaultResult, FaultInjector};
use super::{bit_flip, bit_set, bit_clear, busy_delay_us, Lfsr, should_inject};

pub struct I2cBus {
    pub sda: bool,
    pub scl: bool,
}

pub struct I2cFaultInjector<'d> {
    config: FaultConfig,
    lfsr: Lfsr,
    armed: bool,
    count: u32,
    _lifetime: core::marker::PhantomData<&'d ()>,
}

impl<'d> I2cFaultInjector<'d> {
    pub fn new() -> Self {
        Self {
            config: FaultConfig::new(shared::Protocol::I2c, FaultType::NackInjection),
            lfsr: Lfsr::new(0xCAFE),
            armed: false,
            count: 0,
            _lifetime: core::marker::PhantomData,
        }
    }

    pub fn inject_address(&mut self, addr: u8) -> u8 {
        if !self.armed || !should_inject(&mut self.lfsr, self.config.probability_permille) {
            return addr;
        }
        let bit = if self.config.target_bit < 7 {
            self.config.target_bit
        } else {
            self.lfsr.next_bit() % 7
        };
        let result = match self.config.fault_type {
            FaultType::BitFlip => bit_flip(addr, bit),
            FaultType::StuckAtZero => bit_clear(addr, bit),
            FaultType::StuckAtOne => bit_set(addr, bit),
            _ => addr,
        };
        if result != addr { self.count += 1; }
        result
    }

    pub fn inject_data(&mut self, byte: u8) -> u8 {
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
            _ => byte,
        };
        if result != byte { self.count += 1; }
        result
    }

    pub fn should_nack(&self) -> bool {
        self.armed && self.config.fault_type == FaultType::NackInjection
            && should_inject(&mut Lfsr::new(0), self.config.probability_permille)
    }

    pub fn should_lock_bus(&self) -> bool {
        self.armed && self.config.fault_type == FaultType::BusLockup
    }

    pub fn inject_bus_lockup(&self) {
        if self.should_lock_bus() {
            busy_delay_us(self.config.duration_us);
        }
    }
}

impl<'d, 'b> FaultInjector<'d, I2cBus> for I2cFaultInjector<'d> {
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

    fn fire(&mut self, bus: &mut I2cBus) -> Result<FaultResult, ()> {
        if !self.armed { return Err(()); }
        if self.config.fault_type == FaultType::BusLockup {
            self.inject_bus_lockup();
        }
        if self.config.fault_type == FaultType::NackInjection {
            self.count += 1;
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
    fn i2c_bit_flip_address() {
        let cfg = FaultConfig::new(shared::Protocol::I2c, FaultType::BitFlip).at_bit(0);
        let mut inj = I2cFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert_eq!(inj.inject_address(0x50), 0x51);
    }

    #[test]
    fn i2c_stuck_at_zero_data() {
        let cfg = FaultConfig::new(shared::Protocol::I2c, FaultType::StuckAtZero).at_bit(3);
        let mut inj = I2cFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert_eq!(inj.inject_data(0xFF), 0xF7);
    }

    #[test]
    fn i2c_bus_lockup_detection() {
        let cfg = FaultConfig::new(shared::Protocol::I2c, FaultType::BusLockup);
        let mut inj = I2cFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert!(inj.should_lock_bus());
    }

    #[test]
    fn i2c_no_nack_when_bitflip() {
        let cfg = FaultConfig::new(shared::Protocol::I2c, FaultType::BitFlip);
        let mut inj = I2cFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert!(!inj.should_lock_bus());
    }

    #[test]
    fn i2c_trait_fire() {
        let cfg = FaultConfig::new(shared::Protocol::I2c, FaultType::NackInjection);
        let mut inj = I2cFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        let mut bus = I2cBus { sda: true, scl: true };
        assert_eq!(inj.fire(&mut bus).unwrap(), FaultResult::Fired);
    }
}
