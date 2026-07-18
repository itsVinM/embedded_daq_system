use defmt::info;
use shared::{FaultType, FaultConfig};
use super::{bit_flip, bit_set, bit_clear, busy_delay, Lfsr};

pub struct I2cFaultInjector {
    config: FaultConfig,
    lfsr: Lfsr,
    injected_count: u32,
}

impl I2cFaultInjector {
    pub fn new(config: FaultConfig) -> Self {
        Self {
            config,
            lfsr: Lfsr::new(0xCAFE),
            injected_count: 0,
        }
    }

    pub fn inject_address(&mut self, addr: u8, read: bool) -> (u8, bool) {
        if !self.should_inject() {
            return (addr, read);
        }

        let result_addr = match self.config.fault_type {
            FaultType::BitFlip => {
                let bit = if self.config.target_bit < 7 {
                    self.config.target_bit
                } else {
                    self.lfsr.next_bit() % 7
                };
                bit_flip(addr, bit)
            }
            FaultType::StuckAtZero => {
                let bit = if self.config.target_bit < 7 {
                    self.config.target_bit
                } else {
                    self.lfsr.next_bit() % 7
                };
                bit_clear(addr, bit)
            }
            FaultType::StuckAtOne => {
                let bit = if self.config.target_bit < 7 {
                    self.config.target_bit
                } else {
                    self.lfsr.next_bit() % 7
                };
                bit_set(addr, bit)
            }
            _ => addr,
        };

        self.injected_count += 1;
        info!("i2c: fault addr 0x{:02X} → 0x{:02X}", addr, result_addr);
        (result_addr, read)
    }

    pub fn inject_data(&mut self, byte: u8) -> u8 {
        if !self.should_inject() {
            return byte;
        }

        let result = match self.config.fault_type {
            FaultType::BitFlip => {
                let bit = if self.config.target_bit < 8 {
                    self.config.target_bit
                } else {
                    self.lfsr.next_bit()
                };
                bit_flip(byte, bit)
            }
            FaultType::StuckAtZero => {
                let bit = if self.config.target_bit < 8 {
                    self.config.target_bit
                } else {
                    self.lfsr.next_bit()
                };
                bit_clear(byte, bit)
            }
            FaultType::StuckAtOne => {
                let bit = if self.config.target_bit < 8 {
                    self.config.target_bit
                } else {
                    self.lfsr.next_bit()
                };
                bit_set(byte, bit)
            }
            _ => byte,
        };

        self.injected_count += 1;
        info!("i2c: fault data 0x{:02X} → 0x{:02X}", byte, result);
        result
    }

    pub fn should_nack(&self) -> bool {
        if !self.should_inject() {
            return false;
        }
        self.config.fault_type == FaultType::NackInjection
    }

    pub fn should_lock_bus(&self) -> bool {
        if !self.should_inject() {
            return false;
        }
        self.config.fault_type == FaultType::BusLockup
    }

    pub fn inject_scl_hold(&self) {
        if self.config.fault_type == FaultType::BusLockup && self.should_inject() {
            busy_delay(self.config.duration_us * 21);
            info!("i2c: SCL held low (bus lockup, {} us)", self.config.duration_us);
        }
    }

    pub fn inject_sda_delay(&self) {
        if self.config.fault_type == FaultType::BitDelay && self.should_inject() {
            busy_delay(self.config.duration_us * 21);
            info!("i2c: SDA delay ({} us)", self.config.duration_us);
        }
    }

    fn should_inject(&self) -> bool {
        if self.config.probability_permille == 0 {
            return false;
        }
        if self.config.probability_permille >= 1000 {
            return true;
        }
        let r = self.lfsr.next() & 0x03FF;
        (r as u16) < self.config.probability_permille
    }

    pub fn injected_count(&self) -> u32 {
        self.injected_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i2c_bit_flip_address() {
        let cfg = FaultConfig::new(shared::Protocol::I2c, FaultType::BitFlip).at_bit(0);
        let mut inj = I2cFaultInjector::new(cfg);
        let (addr, _) = inj.inject_address(0x50, false);
        assert_eq!(addr, 0x51);
    }

    #[test]
    fn i2c_nack_detection() {
        let cfg = FaultConfig::new(shared::Protocol::I2c, FaultType::NackInjection);
        let inj = I2cFaultInjector::new(cfg);
        assert!(inj.should_nack());
    }

    #[test]
    fn i2c_bus_lockup() {
        let cfg = FaultConfig::new(shared::Protocol::I2c, FaultType::BusLockup);
        let inj = I2cFaultInjector::new(cfg);
        assert!(inj.should_lock_bus());
    }

    #[test]
    fn i2c_no_nack_when_bitflip() {
        let cfg = FaultConfig::new(shared::Protocol::I2c, FaultType::BitFlip);
        let inj = I2cFaultInjector::new(cfg);
        assert!(!inj.should_nack());
        assert!(!inj.should_lock_bus());
    }

    #[test]
    fn i2c_data_injection() {
        let cfg = FaultConfig::new(shared::Protocol::I2c, FaultType::StuckAtZero).at_bit(3);
        let mut inj = I2cFaultInjector::new(cfg);
        let result = inj.inject_data(0xFF);
        assert_eq!(result, 0xF7);
    }
}
