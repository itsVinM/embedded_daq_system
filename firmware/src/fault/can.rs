use defmt::info;
use shared::{FaultType, FaultConfig};
use super::{bit_flip, bit_set, bit_clear, busy_delay, Lfsr};

pub struct CanFaultInjector {
    config: FaultConfig,
    lfsr: Lfsr,
    injected_count: u32,
}

impl CanFaultInjector {
    pub fn new(config: FaultConfig) -> Self {
        Self {
            config,
            lfsr: Lfsr::new(0x1234),
            injected_count: 0,
        }
    }

    pub fn inject_id(&mut self, id: u32) -> u32 {
        if !self.should_inject() {
            return id;
        }

        let result = match self.config.fault_type {
            FaultType::BitFlip => {
                let bit = if self.config.target_bit < 29 {
                    self.config.target_bit
                } else {
                    self.lfsr.next_bit() % 29
                };
                id ^ (1 << bit)
            }
            FaultType::StuckAtZero => {
                let bit = if self.config.target_bit < 29 {
                    self.config.target_bit
                } else {
                    self.lfsr.next_bit() % 29
                };
                id & !(1 << bit)
            }
            FaultType::StuckAtOne => {
                let bit = if self.config.target_bit < 29 {
                    self.config.target_bit
                } else {
                    self.lfsr.next_bit() % 29
                };
                id | (1 << bit)
            }
            _ => id,
        };

        self.injected_count += 1;
        info!("can: fault ID 0x{:08X} → 0x{:08X}", id, result);
        result
    }

    pub fn inject_data(&mut self, byte: u8, index: usize) -> u8 {
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
            FaultType::CrcCorrupt => {
                if index >= 4 {
                    let bit = self.lfsr.next_bit();
                    bit_flip(byte, bit)
                } else {
                    byte
                }
            }
            FaultType::FrameCorrupt => {
                let bit = self.lfsr.next_bit();
                bit_flip(byte, bit)
            }
            _ => byte,
        };

        self.injected_count += 1;
        info!("can: fault data[{}] 0x{:02X} → 0x{:02X}", index, byte, result);
        result
    }

    pub fn inject_stuff_bit(&mut self) -> bool {
        if !self.should_inject() {
            return false;
        }
        if self.config.fault_type == FaultType::ClockGlitch {
            self.injected_count += 1;
            info!("can: stuff bit error injected");
            return true;
        }
        false
    }

    pub fn inject_bit_timing(&self) {
        if self.config.fault_type == FaultType::BitDelay && self.should_inject() {
            busy_delay(self.config.duration_us * 21);
            info!("can: bit timing offset ({} us)", self.config.duration_us);
        }
    }

    pub fn inject_form_error(&self) -> bool {
        if !self.should_inject() {
            return false;
        }
        if self.config.fault_type == FaultType::FrameCorrupt {
            info!("can: form error injected");
            return true;
        }
        false
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
    fn can_bit_flip_id() {
        let cfg = FaultConfig::new(shared::Protocol::Can, FaultType::BitFlip).at_bit(0);
        let mut inj = CanFaultInjector::new(cfg);
        let result = inj.inject_id(0x0000_0000);
        assert_eq!(result, 0x0000_0001);
    }

    #[test]
    fn can_stuck_at_zero_id() {
        let cfg = FaultConfig::new(shared::Protocol::Can, FaultType::StuckAtZero).at_bit(3);
        let mut inj = CanFaultInjector::new(cfg);
        let result = inj.inject_id(0xFFFF_FFFF);
        assert_eq!(result, 0xFFFF_FFF7);
    }

    #[test]
    fn can_crc_corrupt() {
        let cfg = FaultConfig::new(shared::Protocol::Can, FaultType::CrcCorrupt);
        let mut inj = CanFaultInjector::new(cfg);
        let d0 = inj.inject_data(0xFF, 0);
        let d4 = inj.inject_data(0xFF, 4);
        assert_eq!(d0, 0xFF);
        assert_ne!(d4, 0xFF);
    }

    #[test]
    fn can_form_error() {
        let cfg = FaultConfig::new(shared::Protocol::Can, FaultType::FrameCorrupt);
        let inj = CanFaultInjector::new(cfg);
        assert!(inj.inject_form_error());
    }

    #[test]
    fn can_stuff_bit_error() {
        let cfg = FaultConfig::new(shared::Protocol::Can, FaultType::ClockGlitch);
        let mut inj = CanFaultInjector::new(cfg);
        assert!(inj.inject_stuff_bit());
    }
}
