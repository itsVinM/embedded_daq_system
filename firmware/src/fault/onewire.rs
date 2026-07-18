use defmt::info;
use shared::{FaultType, FaultConfig};
use super::{bit_flip, bit_set, bit_clear, busy_delay, Lfsr};

pub struct OneWireFaultInjector {
    config: FaultConfig,
    lfsr: Lfsr,
    injected_count: u32,
}

impl OneWireFaultInjector {
    pub fn new(config: FaultConfig) -> Self {
        Self {
            config,
            lfsr: Lfsr::new(0x5678),
            injected_count: 0,
        }
    }

    pub fn inject_presence(&self) -> bool {
        if !self.should_inject() {
            return false;
        }
        if self.config.fault_type == FaultType::Timeout {
            busy_delay(self.config.duration_us * 21);
            info!("onewire: presence pulse suppressed ({} us)", self.config.duration_us);
            return true;
        }
        false
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
            FaultType::CrcCorrupt => {
                if index >= 8 {
                    let bit = self.lfsr.next_bit();
                    bit_flip(byte, bit)
                } else {
                    byte
                }
            }
            _ => byte,
        };

        self.injected_count += 1;
        info!("onewire: fault data[{}] 0x{:02X} → 0x{:02X}", index, byte, result);
        result
    }

    pub fn inject_timing(&self) {
        if self.config.fault_type == FaultType::BitDelay && self.should_inject() {
            busy_delay(self.config.duration_us * 21);
            info!("onewire: timing violation ({} us)", self.config.duration_us);
        }
    }

    pub fn inject_reset_pulse(&self) -> bool {
        if !self.should_inject() {
            return false;
        }
        if self.config.fault_type == FaultType::ClockGlitch {
            busy_delay(100);
            info!("onewire: reset pulse glitch");
            return true;
        }
        false
    }

    pub fn inject_rom_command(&mut self, cmd: u8) -> u8 {
        if !self.should_inject() {
            return cmd;
        }

        let result = match self.config.fault_type {
            FaultType::BitFlip => {
                let bit = if self.config.target_bit < 8 {
                    self.config.target_bit
                } else {
                    self.lfsr.next_bit()
                };
                bit_flip(cmd, bit)
            }
            _ => cmd,
        };

        self.injected_count += 1;
        info!("onewire: ROM cmd 0x{:02X} → 0x{:02X}", cmd, result);
        result
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
    fn onewire_bit_flip() {
        let cfg = FaultConfig::new(shared::Protocol::OneWire, FaultType::BitFlip).at_bit(0);
        let mut inj = OneWireFaultInjector::new(cfg);
        let result = inj.inject_data(0b1111_1110, 0);
        assert_eq!(result, 0b1111_1111);
    }

    #[test]
    fn onewire_crc_corrupt_only_after_index8() {
        let cfg = FaultConfig::new(shared::Protocol::OneWire, FaultType::CrcCorrupt);
        let mut inj = OneWireFaultInjector::new(cfg);
        let d0 = inj.inject_data(0xFF, 0);
        let d10 = inj.inject_data(0xFF, 10);
        assert_eq!(d0, 0xFF);
        assert_ne!(d10, 0xFF);
    }

    #[test]
    fn onewire_presence_suppressed() {
        let cfg = FaultConfig::new(shared::Protocol::OneWire, FaultType::Timeout);
        let inj = OneWireFaultInjector::new(cfg);
        assert!(inj.inject_presence());
    }

    #[test]
    fn onewire_reset_glitch() {
        let cfg = FaultConfig::new(shared::Protocol::OneWire, FaultType::ClockGlitch);
        let mut inj = OneWireFaultInjector::new(cfg);
        assert!(inj.inject_reset_pulse());
    }

    #[test]
    fn onewire_rom_command_flip() {
        let cfg = FaultConfig::new(shared::Protocol::OneWire, FaultType::BitFlip).at_bit(0);
        let mut inj = OneWireFaultInjector::new(cfg);
        let result = inj.inject_rom_command(0xCC);
        assert_eq!(result, 0xCD);
    }
}
