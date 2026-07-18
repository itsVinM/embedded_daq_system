pub mod spi;
pub mod i2c;
pub mod uart;
pub mod can;
pub mod onewire;

use defmt::info;
use shared::{FaultConfig, FaultType, FaultCommand, FaultResult, Protocol};

pub struct FaultEngine {
    config: FaultConfig,
    armed: bool,
    triggered: bool,
    trigger_count: u32,
}

impl FaultEngine {
    pub fn new() -> Self {
        Self {
            config: FaultConfig::new(Protocol::Spi, FaultType::BitFlip),
            armed: false,
            triggered: false,
            trigger_count: 0,
        }
    }

    pub fn handle_command(&mut self, cmd: FaultCommand, config: Option<FaultConfig>) -> FaultResult {
        match cmd {
            FaultCommand::Arm => {
                if let Some(cfg) = config {
                    self.config = cfg;
                }
                self.armed = true;
                self.triggered = false;
                self.trigger_count = 0;
                info!("fault: armed protocol={} type={}", self.config.protocol as u8, self.config.fault_type as u8);
                FaultResult::Armed
            }
            FaultCommand::Disarm => {
                self.armed = false;
                info!("fault: disarmed");
                FaultResult::Disarmed
            }
            FaultCommand::Trigger => {
                if !self.armed {
                    return FaultResult::Error;
                }
                self.triggered = true;
                self.trigger_count += 1;
                info!("fault: triggered #{}", self.trigger_count);
                FaultResult::Triggered
            }
            FaultCommand::Status => {
                if self.triggered {
                    FaultResult::Completed
                } else if self.armed {
                    FaultResult::Armed
                } else {
                    FaultResult::Disarmed
                }
            }
            FaultCommand::Reset => {
                self.armed = false;
                self.triggered = false;
                self.trigger_count = 0;
                FaultResult::Disarmed
            }
        }
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }

    pub fn should_inject(&self) -> bool {
        self.armed && self.triggered
    }

    pub fn config(&self) -> &FaultConfig {
        &self.config
    }

    pub fn trigger_count(&self) -> u32 {
        self.trigger_count
    }
}

/// Apply a bit-flip to `byte` at position `bit` (0 = LSB).
#[inline(always)]
pub fn bit_flip(byte: u8, bit: u8) -> u8 {
    byte ^ (1 << bit)
}

/// Force bit to 1.
#[inline(always)]
pub fn bit_set(byte: u8, bit: u8) -> u8 {
    byte | (1 << bit)
}

/// Force bit to 0.
#[inline(always)]
pub fn bit_clear(byte: u8, bit: u8) -> u8 {
    byte & !(1 << bit)
}

/// Inject delay in busy-loop cycles (approximate, assumes ~4 cycles/instruction at 84 MHz).
#[inline(always)]
pub fn busy_delay(cycles: u32) {
    for _ in 0..cycles {
        cortex_m::asm::nop();
    }
}

/// Simple LFSR for pseudo-random bit selection.
pub struct Lfsr {
    state: u16,
}

impl Lfsr {
    pub fn new(seed: u16) -> Self {
        Self { state: if seed == 0 { 1 } else { seed } }
    }

    pub fn next(&mut self) -> u16 {
        let bit = ((self.state >> 0) ^ (self.state >> 2) ^ (self.state >> 3) ^ (self.state >> 5)) & 1;
        self.state = (self.state >> 1) | (bit << 15);
        self.state
    }

    pub fn next_bit(&mut self) -> u8 {
        (self.next() & 0x07) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_flip_toggles() {
        assert_eq!(bit_flip(0b1010_0000, 0), 0b1010_0001);
        assert_eq!(bit_flip(0b1010_0001, 0), 0b1010_0000);
        assert_eq!(bit_flip(0b0000_0000, 7), 0b1000_0000);
    }

    #[test]
    fn bit_set_forces_high() {
        assert_eq!(bit_set(0b0000_0000, 3), 0b0000_1000);
        assert_eq!(bit_set(0b0000_1000, 3), 0b0000_1000);
    }

    #[test]
    fn bit_clear_forces_low() {
        assert_eq!(bit_clear(0b1111_1111, 4), 0b1110_1111);
        assert_eq!(bit_clear(0b0000_0000, 4), 0b0000_0000);
    }

    #[test]
    fn lfsr_produces_values() {
        let mut lfsr = Lfsr::new(0xACE1);
        let v1 = lfsr.next();
        let v2 = lfsr.next();
        assert_ne!(v1, 0);
        assert_ne!(v2, 0);
        assert_ne!(v1, v2);
    }

    #[test]
    fn lfsr_next_bit_range() {
        let mut lfsr = Lfsr::new(42);
        for _ in 0..100 {
            let bit = lfsr.next_bit();
            assert!(bit < 8);
        }
    }

    #[test]
    fn engine_arm_disarm() {
        let mut engine = FaultEngine::new();
        assert!(!engine.is_armed());
        let r = engine.handle_command(FaultCommand::Arm, None);
        assert_eq!(r, FaultResult::Armed);
        assert!(engine.is_armed());
        let r = engine.handle_command(FaultCommand::Disarm, None);
        assert_eq!(r, FaultResult::Disarmed);
        assert!(!engine.is_armed());
    }

    #[test]
    fn engine_trigger_when_armed() {
        let mut engine = FaultEngine::new();
        engine.handle_command(FaultCommand::Arm, None);
        let r = engine.handle_command(FaultCommand::Trigger, None);
        assert_eq!(r, FaultResult::Triggered);
        assert!(engine.should_inject());
        assert_eq!(engine.trigger_count(), 1);
    }

    #[test]
    fn engine_trigger_when_disarmed() {
        let mut engine = FaultEngine::new();
        let r = engine.handle_command(FaultCommand::Trigger, None);
        assert_eq!(r, FaultResult::Error);
        assert!(!engine.should_inject());
    }

    #[test]
    fn engine_reset() {
        let mut engine = FaultEngine::new();
        engine.handle_command(FaultCommand::Arm, None);
        engine.handle_command(FaultCommand::Trigger, None);
        let r = engine.handle_command(FaultCommand::Reset, None);
        assert_eq!(r, FaultResult::Disarmed);
        assert!(!engine.is_armed());
        assert!(!engine.should_inject());
        assert_eq!(engine.trigger_count(), 0);
    }

    #[test]
    fn engine_status() {
        let mut engine = FaultEngine::new();
        assert_eq!(engine.handle_command(FaultCommand::Status, None), FaultResult::Disarmed);
        engine.handle_command(FaultCommand::Arm, None);
        assert_eq!(engine.handle_command(FaultCommand::Status, None), FaultResult::Armed);
        engine.handle_command(FaultCommand::Trigger, None);
        assert_eq!(engine.handle_command(FaultCommand::Status, None), FaultResult::Completed);
    }
}
