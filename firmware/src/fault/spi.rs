use defmt::info;
use shared::{FaultType, FaultConfig};
use super::{bit_flip, bit_set, bit_clear, busy_delay, Lfsr};

pub struct SpiFaultInjector {
    config: FaultConfig,
    lfsr: Lfsr,
    injected_count: u32,
}

impl SpiFaultInjector {
    pub fn new(config: FaultConfig) -> Self {
        Self {
            config,
            lfsr: Lfsr::new(0xBEEF),
            injected_count: 0,
        }
    }

    pub fn inject_mosi(&mut self, byte: u8) -> u8 {
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
            FaultType::BitDelay => {
                busy_delay(self.config.duration_us * 21);
                byte
            }
            FaultType::ClockGlitch => {
                busy_delay(5);
                bit_flip(byte, self.lfsr.next_bit())
            }
            _ => byte,
        };

        self.injected_count += 1;
        info!("spi: fault injected MOSI 0x{:02X} → 0x{:02X}", byte, result);
        result
    }

    pub fn inject_miso(&mut self, byte: u8) -> u8 {
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
        info!("spi: fault injected MISO 0x{:02X} → 0x{:02X}", byte, result);
        result
    }

    pub fn inject_clock(&mut self) {
        if !self.should_inject() {
            return;
        }
        if self.config.fault_type == FaultType::ClockGlitch {
            busy_delay(self.config.duration_us * 21);
            info!("spi: clock glitch injected ({} us)", self.config.duration_us);
        }
    }

    pub fn inject_cs_hold(&mut self) {
        if !self.should_inject() {
            return;
        }
        if self.config.fault_type == FaultType::Timeout {
            busy_delay(self.config.duration_us * 21);
            info!("spi: CS hold extended ({} us)", self.config.duration_us);
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
    fn spi_bit_flip() {
        let cfg = FaultConfig::new(shared::Protocol::Spi, FaultType::BitFlip).at_bit(0);
        let mut inj = SpiFaultInjector::new(cfg);
        let result = inj.inject_mosi(0b1010_0000);
        assert_eq!(result, 0b1010_0001);
    }

    #[test]
    fn spi_stuck_at_zero() {
        let cfg = FaultConfig::new(shared::Protocol::Spi, FaultType::StuckAtZero).at_bit(7);
        let mut inj = SpiFaultInjector::new(cfg);
        let result = inj.inject_mosi(0b1111_1111);
        assert_eq!(result, 0b0111_1111);
    }

    #[test]
    fn spi_stuck_at_one() {
        let cfg = FaultConfig::new(shared::Protocol::Spi, FaultType::StuckAtOne).at_bit(3);
        let mut inj = SpiFaultInjector::new(cfg);
        let result = inj.inject_mosi(0b0000_0000);
        assert_eq!(result, 0b0000_1000);
    }

    #[test]
    fn spi_zero_probability_no_inject() {
        let cfg = FaultConfig::new(shared::Protocol::Spi, FaultType::BitFlip)
            .probability(0);
        let mut inj = SpiFaultInjector::new(cfg);
        let result = inj.inject_mosi(0xAA);
        assert_eq!(result, 0xAA);
        assert_eq!(inj.injected_count(), 0);
    }
}
