pub mod spi;
pub mod i2c;
pub mod uart;
pub mod can;
pub mod onewire;

use defmt::info;
use shared::{FaultConfig, FaultType, FaultCommand, FaultResult, Protocol, FaultInjector};

// ─── Bit manipulation primitives ──────────────────────────────────────────────

#[inline(always)]
pub fn bit_flip(byte: u8, bit: u8) -> u8 {
    byte ^ (1 << (bit & 7))
}

#[inline(always)]
pub fn bit_set(byte: u8, bit: u8) -> u8 {
    byte | (1 << (bit & 7))
}

#[inline(always)]
pub fn bit_clear(byte: u8, bit: u8) -> u8 {
    byte & !(1 << (bit & 7))
}

#[inline(always)]
pub fn busy_delay_us(us: u32) {
    let cycles = us.wrapping_mul(21);
    for _ in 0..cycles {
        cortex_m::asm::nop();
    }
}

// ─── LFSR for probabilistic injection ────────────────────────────────────────

pub struct Lfsr {
    state: u16,
}

impl Lfsr {
    pub const fn new(seed: u16) -> Self {
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

#[inline(always)]
pub fn should_inject(lfsr: &mut Lfsr, permille: u16) -> bool {
    if permille == 0 {
        return false;
    }
    if permille >= 1000 {
        return true;
    }
    let r = lfsr.next() & 0x03FF;
    (r as u16) < permille
}

// ─── Fault Engine (pure state, no hardware) ───────────────────────────────────

pub struct FaultEngine {
    config: FaultConfig,
    armed: bool,
    fired: bool,
    fire_count: u32,
}

impl FaultEngine {
    pub fn new() -> Self {
        Self {
            config: FaultConfig::new(Protocol::Spi, FaultType::BitFlip),
            armed: false,
            fired: false,
            fire_count: 0,
        }
    }

    pub fn with_config(mut self, config: FaultConfig) -> Self {
        self.config = config;
        self
    }

    pub fn handle_command(&mut self, cmd: FaultCommand) -> FaultResult {
        match cmd {
            FaultCommand::Arm => {
                self.armed = true;
                self.fired = false;
                self.fire_count = 0;
                FaultResult::Armed
            }
            FaultCommand::Disarm => {
                self.armed = false;
                FaultResult::Disarmed
            }
            FaultCommand::Fire => {
                if !self.armed {
                    return FaultResult::Error;
                }
                self.fired = true;
                self.fire_count += 1;
                FaultResult::Fired
            }
            FaultCommand::Status => {
                if self.fired { FaultResult::Completed }
                else if self.armed { FaultResult::Armed }
                else { FaultResult::Disarmed }
            }
            FaultCommand::Reset => {
                self.armed = false;
                self.fired = false;
                self.fire_count = 0;
                FaultResult::Disarmed
            }
        }
    }

    pub fn is_armed(&self) -> bool { self.armed }
    pub fn should_inject(&self) -> bool { self.armed && self.fired }
    pub fn config(&self) -> &FaultConfig { &self.config }
    pub fn fire_count(&self) -> u32 { self.fire_count }
    pub fn reset_stats(&mut self) { self.fire_count = 0; }
}

// ─── Bit-banged UART command interface (8N1, 115200) ──────────────────────────

#[cfg(feature = "fault")]
use embassy_stm32::gpio::{Level, Output, Speed, Input, Pull};
#[cfg(feature = "fault")]
use embassy_stm32::Peri;
#[cfg(feature = "fault")]
use embassy_stm32::peripherals::{PB6, PB7};

pub struct UartBitbang<'d> {
    #[cfg(feature = "fault")]
    tx: Output<'d>,
    #[cfg(feature = "fault")]
    input: Input<'d>,
    baud_period_us: u32,
    _phantom: core::marker::PhantomData<&'d ()>,
}

#[cfg(feature = "fault")]
impl<'d> UartBitbang<'d> {
    pub fn new(tx_pin: Peri<'d, PB6>, rx_pin: Peri<'d, PB7>) -> Self {
        Self {
            tx: Output::new(tx_pin, Level::High, Speed::High),
            input: Input::new(rx_pin, Pull::Up),
            baud_period_us: 8,
            _phantom: core::marker::PhantomData,
        }
    }

    fn bit_delay(&self) {
        busy_delay_us(self.baud_period_us);
    }

    pub async fn write_byte(&mut self, byte: u8) {
        self.tx.set_low();
        self.bit_delay();
        for i in 0..8u8 {
            if (byte >> i) & 1 == 1 { self.tx.set_high(); }
            else { self.tx.set_low(); }
            self.bit_delay();
        }
        self.tx.set_high();
        self.bit_delay();
    }

    pub async fn write(&mut self, data: &[u8]) {
        for &byte in data {
            self.write_byte(byte).await;
        }
    }

    pub async fn read_byte(&mut self) -> u8 {
        while self.input.is_high() {}
        self.bit_delay();
        self.bit_delay();
        let mut byte = 0u8;
        for i in 0..8u8 {
            if self.input.is_high() { byte |= 1 << i; }
            self.bit_delay();
        }
        byte
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> Result<(), ()> {
        for byte in buf.iter_mut() {
            *byte = self.read_byte().await;
        }
        Ok(())
    }
}

/// Embassy task entry point for fault injection command loop.
#[cfg(feature = "fault")]
#[embassy_executor::task]
pub async fn fault_task_entry(uart: UartBitbang<'static>, engine: FaultEngine) {
    info!("fault: task entry started");
    let mut uart = uart;
    let mut engine = engine;
    let mut buf = [0u8; 1];

    loop {
        let _ = uart.read(&mut buf).await;
        let cmd = match buf[0] {
            0x01 => FaultCommand::Arm,
            0x02 => FaultCommand::Disarm,
            0x03 => FaultCommand::Fire,
            0x04 => FaultCommand::Status,
            0x05 => FaultCommand::Reset,
            _ => continue,
        };
        let result = engine.handle_command(cmd);
        let _ = uart.write(&[result as u8]).await;
        info!("fault: cmd={} result={}", buf[0], result as u8);
    }
}

// ─── Tests (no hardware dependency) ──────────────────────────────────────────

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
    fn bit_wraps_on_overflow() {
        assert_eq!(bit_flip(0xFF, 8), 0xFE);
        assert_eq!(bit_flip(0xFF, 16), 0xFE);
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
            assert!(lfsr.next_bit() < 8);
        }
    }

    #[test]
    fn should_inject_full_permille() {
        let mut lfsr = Lfsr::new(1);
        for _ in 0..1000 {
            assert!(should_inject(&mut lfsr, 1000));
        }
    }

    #[test]
    fn should_inject_zero_permille() {
        let mut lfsr = Lfsr::new(1);
        for _ in 0..1000 {
            assert!(!should_inject(&mut lfsr, 0));
        }
    }

    #[test]
    fn engine_arm_disarm() {
        let mut engine = FaultEngine::new();
        assert!(!engine.is_armed());
        assert_eq!(engine.handle_command(FaultCommand::Arm), FaultResult::Armed);
        assert!(engine.is_armed());
        assert_eq!(engine.handle_command(FaultCommand::Disarm), FaultResult::Disarmed);
        assert!(!engine.is_armed());
    }

    #[test]
    fn engine_fire_when_armed() {
        let mut engine = FaultEngine::new()
            .with_config(FaultConfig::new(Protocol::I2c, FaultType::NackInjection));
        engine.handle_command(FaultCommand::Arm);
        assert_eq!(engine.handle_command(FaultCommand::Fire), FaultResult::Fired);
        assert!(engine.should_inject());
        assert_eq!(engine.fire_count(), 1);
    }

    #[test]
    fn engine_fire_when_disarmed() {
        let mut engine = FaultEngine::new();
        assert_eq!(engine.handle_command(FaultCommand::Fire), FaultResult::Error);
    }

    #[test]
    fn engine_status() {
        let mut engine = FaultEngine::new();
        assert_eq!(engine.handle_command(FaultCommand::Status), FaultResult::Disarmed);
        engine.handle_command(FaultCommand::Arm);
        assert_eq!(engine.handle_command(FaultCommand::Status), FaultResult::Armed);
        engine.handle_command(FaultCommand::Fire);
        assert_eq!(engine.handle_command(FaultCommand::Status), FaultResult::Completed);
    }

    #[test]
    fn engine_reset() {
        let mut engine = FaultEngine::new();
        engine.handle_command(FaultCommand::Arm);
        engine.handle_command(FaultCommand::Fire);
        assert_eq!(engine.handle_command(FaultCommand::Reset), FaultResult::Disarmed);
        assert!(!engine.is_armed());
        assert!(!engine.should_inject());
        assert_eq!(engine.fire_count(), 0);
    }

    #[test]
    fn engine_config_override() {
        let engine = FaultEngine::new()
            .with_config(FaultConfig::new(Protocol::Can, FaultType::BitFlip).at_bit(5));
        assert_eq!(engine.config().protocol, Protocol::Can);
        assert_eq!(engine.config().target_bit, 5);
    }
}
