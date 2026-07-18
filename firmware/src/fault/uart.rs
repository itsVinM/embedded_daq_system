use defmt::info;
use shared::{FaultType, FaultConfig};
use super::{bit_flip, bit_set, bit_clear, busy_delay, Lfsr};

pub struct UartFaultInjector {
    config: FaultConfig,
    lfsr: Lfsr,
    injected_count: u32,
    overrun_active: bool,
}

impl UartFaultInjector {
    pub fn new(config: FaultConfig) -> Self {
        Self {
            config,
            lfsr: Lfsr::new(0xDEAD),
            injected_count: 0,
            overrun_active: false,
        }
    }

    pub fn inject_tx(&mut self, byte: u8) -> UartTxAction {
        if !self.should_inject() {
            return UartTxAction::Send(byte);
        }

        match self.config.fault_type {
            FaultType::BitFlip => {
                let bit = if self.config.target_bit < 8 {
                    self.config.target_bit
                } else {
                    self.lfsr.next_bit()
                };
                let result = bit_flip(byte, bit);
                self.injected_count += 1;
                info!("uart: TX fault 0x{:02X} → 0x{:02X}", byte, result);
                UartTxAction::Send(result)
            }
            FaultType::ParityError => {
                let parity_bit = self.lfsr.next_bit() % 2 == 0;
                self.injected_count += 1;
                info!("uart: parity error injected on 0x{:02X}", byte);
                UartTxAction::SendWithParity(byte, parity_bit)
            }
            FaultType::FrameCorrupt => {
                self.injected_count += 1;
                info!("uart: frame corruption on 0x{:02X}", byte);
                UartTxAction::CorruptFrame(byte)
            }
            FaultType::BitDelay => {
                busy_delay(self.config.duration_us * 21);
                self.injected_count += 1;
                info!("uart: TX delay ({} us)", self.config.duration_us);
                UartTxAction::Send(byte)
            }
            FaultType::ClockGlitch => {
                busy_delay(3);
                let result = bit_flip(byte, self.lfsr.next_bit());
                self.injected_count += 1;
                info!("uart: clock glitch on 0x{:02X} → 0x{:02X}", byte, result);
                UartTxAction::Send(result)
            }
            _ => UartTxAction::Send(byte),
        }
    }

    pub fn inject_rx(&mut self, byte: u8) -> u8 {
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
        info!("uart: RX fault 0x{:02X} → 0x{:02X}", byte, result);
        result
    }

    pub fn inject_overrun(&mut self) -> bool {
        if self.config.fault_type == FaultType::Overrun && self.should_inject() {
            self.overrun_active = true;
            self.injected_count += 1;
            info!("uart: overrun fault active");
            return true;
        }
        false
    }

    pub fn is_overrun_active(&self) -> bool {
        self.overrun_active
    }

    pub fn clear_overrun(&mut self) {
        self.overrun_active = false;
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

pub enum UartTxAction {
    Send(u8),
    SendWithParity(u8, bool),
    CorruptFrame(u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uart_bit_flip_tx() {
        let cfg = FaultConfig::new(shared::Protocol::Uart, FaultType::BitFlip).at_bit(0);
        let mut inj = UartFaultInjector::new(cfg);
        match inj.inject_tx(0b1010_0000) {
            UartTxAction::Send(b) => assert_eq!(b, 0b1010_0001),
            _ => panic!("expected Send"),
        }
    }

    #[test]
    fn uart_stuck_at_zero_rx() {
        let cfg = FaultConfig::new(shared::Protocol::Uart, FaultType::StuckAtZero).at_bit(4);
        let mut inj = UartFaultInjector::new(cfg);
        let result = inj.inject_rx(0xFF);
        assert_eq!(result, 0xEF);
    }

    #[test]
    fn uart_overrun_fault() {
        let cfg = FaultConfig::new(shared::Protocol::Uart, FaultType::Overrun);
        let mut inj = UartFaultInjector::new(cfg);
        assert!(inj.inject_overrun());
        assert!(inj.is_overrun_active());
        inj.clear_overrun();
        assert!(!inj.is_overrun_active());
    }

    #[test]
    fn uart_no_overrun_when_bitflip() {
        let cfg = FaultConfig::new(shared::Protocol::Uart, FaultType::BitFlip);
        let mut inj = UartFaultInjector::new(cfg);
        assert!(!inj.inject_overrun());
    }

    #[test]
    fn uart_frame_corrupt() {
        let cfg = FaultConfig::new(shared::Protocol::Uart, FaultType::FrameCorrupt);
        let mut inj = UartFaultInjector::new(cfg);
        match inj.inject_tx(0x55) {
            UartTxAction::CorruptFrame(b) => assert_eq!(b, 0x55),
            _ => panic!("expected CorruptFrame"),
        }
    }
}
