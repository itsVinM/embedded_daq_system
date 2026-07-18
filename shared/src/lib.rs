#![cfg_attr(not(test), no_std)]

use core::mem::MaybeUninit;

// ─── DMA Buffer ───────────────────────────────────────────────────────────────

#[repr(C, align(4))]
pub struct DmaBuf<T, const N: usize> {
    buf: [MaybeUninit<T>; N],
}

impl<T, const N: usize> DmaBuf<T, N> {
    pub const fn new() -> Self
    where
        T: Copy,
    {
        Self { buf: [MaybeUninit::uninit(); N] }
    }

    pub const fn len(&self) -> usize {
        N
    }

    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.buf.as_mut_ptr() as *mut T
    }

    pub unsafe fn as_mut_slice(&mut self) -> &mut [T] {
        core::slice::from_raw_parts_mut(self.buf.as_mut_ptr() as *mut T, N)
    }
}

// ─── DAQ Data Structures ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ChannelId {
    Analog0 = 0,
    Analog1 = 1,
    PwmIn   = 2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Sample {
    pub channel: ChannelId,
    pub timestamp_us: u32,
    pub value: u32,
    pub flags: u8,
}

pub const SAMPLES_PER_PACKET: usize = 32;

#[repr(C)]
pub struct SamplePacket {
    pub magic: u16,
    pub seq: u16,
    pub count: u16,
    pub samples: [Sample; SAMPLES_PER_PACKET],
}

impl SamplePacket {
    pub const MAGIC: u16 = 0xDA71;

    pub const fn new() -> Self {
        Self {
            magic: Self::MAGIC,
            seq: 0,
            count: 0,
            samples: [Sample {
                channel: ChannelId::Analog0,
                timestamp_us: 0,
                value: 0,
                flags: 0,
            }; SAMPLES_PER_PACKET],
        }
    }

    pub fn push(&mut self, channel: ChannelId, timestamp_us: u32, value: u32) {
        let idx = self.count as usize;
        if idx < SAMPLES_PER_PACKET {
            self.samples[idx] = Sample { channel, timestamp_us, value, flags: 0 };
            self.count += 1;
        }
    }

    pub fn is_full(&self) -> bool {
        self.count as usize >= SAMPLES_PER_PACKET
    }

    pub fn finalize(&mut self, seq: u16) {
        self.magic = Self::MAGIC;
        self.seq = seq;
    }

    pub fn as_bytes(&self) -> &[u8] {
        let len = core::mem::size_of_val(self);
        unsafe { core::slice::from_raw_parts(self as *const _ as *const u8, len) }
    }
}

// ─── Calibration ──────────────────────────────────────────────────────────────

#[repr(C)]
pub struct AdcCalibration {
    pub vref_mv: u16,
    pub offset_ch0: i16,
    pub offset_ch1: i16,
    pub gain_ch0: u16,
    pub gain_ch1: u16,
}

impl AdcCalibration {
    pub const fn default() -> Self {
        Self { vref_mv: 3300, offset_ch0: 0, offset_ch1: 0, gain_ch0: 4095, gain_ch1: 4095 }
    }

    pub fn counts_to_mv(&self, counts: u32, channel: ChannelId) -> u32 {
        let (offset, gain) = match channel {
            ChannelId::Analog0 => (self.offset_ch0 as i32, self.gain_ch0 as u32),
            ChannelId::Analog1 => (self.offset_ch1 as i32, self.gain_ch1 as u32),
            _ => return 0,
        };
        let adjusted = (counts as i32).saturating_add(-offset).max(0) as u32;
        (adjusted * self.vref_mv as u32) / gain
    }
}

// ─── Protocol Commands ────────────────────────────────────────────────────────

#[repr(u8)]
pub enum Command {
    StartStream  = 0x01,
    StopStream   = 0x02,
    SetRate      = 0x03,
    Calibrate    = 0x04,
    Ping         = 0x05,
    QueryConfig  = 0x06,
}

#[repr(u8)]
pub enum Response {
    Ack       = 0x81,
    Nak       = 0x82,
    Data      = 0x83,
    Config    = 0x84,
    Pong      = 0x85,
}

// ─── Acquisition Config ───────────────────────────────────────────────────────

#[repr(C)]
pub struct AcquisitionConfig {
    pub sample_rate_hz: u32,
    pub channels_active: u8,
    pub dma_depth: u16,
}

impl AcquisitionConfig {
    pub const fn default() -> Self {
        Self { sample_rate_hz: 10_000, channels_active: 0x03, dma_depth: 4096 }
    }
}

// ─── Health Status ────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum HealthStatus {
    Ready,
    Fail(HealthError),
}

#[derive(Debug, PartialEq)]
pub enum HealthError {
    StackCanary,
    RamTest,
    FlashCRC,
    TimerNotTicking,
    ClockOutOfRange,
    ClockHclkNotRunning,
    AdcCalibration,
    I2cInitFailed,
    SpiInitFailed,
    UartInitFailed,
    DmaInitFailed,
}

impl HealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthStatus::Ready                                     => "READY\n",
            HealthStatus::Fail(HealthError::AdcCalibration)        => "FAIL:adc\n",
            HealthStatus::Fail(HealthError::TimerNotTicking)        => "FAIL:tim\n",
            HealthStatus::Fail(HealthError::ClockOutOfRange)        => "FAIL:clk\n",
            HealthStatus::Fail(HealthError::ClockHclkNotRunning)   => "FAIL:hclk\n",
            HealthStatus::Fail(HealthError::StackCanary)            => "FAIL:stack\n",
            HealthStatus::Fail(HealthError::RamTest)                => "FAIL:ram\n",
            HealthStatus::Fail(HealthError::FlashCRC)               => "FAIL:flash\n",
            HealthStatus::Fail(HealthError::I2cInitFailed)          => "FAIL:i2c\n",
            HealthStatus::Fail(HealthError::SpiInitFailed)          => "FAIL:spi\n",
            HealthStatus::Fail(HealthError::UartInitFailed)         => "FAIL:uart\n",
            HealthStatus::Fail(HealthError::DmaInitFailed)          => "FAIL:dma\n",
        }
    }
}

// ─── Fault Injection ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Protocol {
    Spi     = 0,
    I2c     = 1,
    Uart    = 2,
    Can     = 3,
    OneWire = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FaultType {
    BitFlip       = 0x01,
    StuckAtZero   = 0x02,
    StuckAtOne    = 0x03,
    BitDelay      = 0x04,
    ClockGlitch   = 0x05,
    FrameCorrupt  = 0x06,
    NackInjection = 0x07,
    ParityError   = 0x08,
    CrcCorrupt    = 0x09,
    BusLockup     = 0x0A,
    Overrun       = 0x0B,
    Timeout       = 0x0C,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FaultCommand {
    Arm    = 0x01,
    Disarm = 0x02,
    Fire   = 0x03,
    Status = 0x04,
    Reset  = 0x05,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FaultResult {
    Armed     = 0x01,
    Disarmed  = 0x02,
    Fired     = 0x03,
    Busy      = 0x04,
    Error     = 0x05,
    Completed = 0x06,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct FaultConfig {
    pub protocol: Protocol,
    pub fault_type: FaultType,
    pub target_bit: u8,
    pub duration_us: u32,
    pub repeat_count: u8,
    pub probability_permille: u16,
}

impl FaultConfig {
    pub const fn new(protocol: Protocol, fault_type: FaultType) -> Self {
        Self {
            protocol,
            fault_type,
            target_bit: 0,
            duration_us: 0,
            repeat_count: 1,
            probability_permille: 1000,
        }
    }

    pub const fn at_bit(mut self, bit: u8) -> Self {
        self.target_bit = bit;
        self
    }

    pub const fn for_us(mut self, us: u32) -> Self {
        self.duration_us = us;
        self
    }

    pub const fn repeat(mut self, n: u8) -> Self {
        self.repeat_count = n;
        self
    }

    pub const fn probability(mut self, permille: u16) -> Self {
        self.probability_permille = permille;
        self
    }
}

/// Core trait for all protocol fault injectors.
/// Generic over the protocol bus type `B` — each protocol provides its own bus handle.
pub trait FaultInjector<'d, B: ?Sized> {
    type Error;

    fn configure(&mut self, config: &FaultConfig) -> Result<(), Self::Error>;
    fn arm(&mut self) -> Result<(), Self::Error>;
    fn disarm(&mut self) -> Result<(), Self::Error>;
    fn fire(&mut self, bus: &mut B) -> Result<FaultResult, Self::Error>;
    fn is_armed(&self) -> bool;
    fn injected_count(&self) -> u32;
    fn reset_stats(&mut self);
}

// ─── Telemetry ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TelemetryChannel {
    Voltage   = 0,
    Current   = 1,
    Temperature = 2,
    Rtc       = 3,
    Gpio      = 4,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TelemetryFrame {
    pub channel: TelemetryChannel,
    pub timestamp_us: u32,
    pub value_i32: i32,
    pub value_u32: u32,
    pub flags: u16,
}

/// Generic telemetry reader trait.
/// `T` is the concrete sensor/driver type — each backend provides its own.
pub trait TelemetryReader<'d, T> {
    type Error;

    fn read_raw(&mut self, sensor: &'d T) -> Result<u32, Self::Error>;
    fn read_calibrated(&mut self, sensor: &'d T) -> Result<TelemetryFrame, Self::Error>;
    fn channel(&self) -> TelemetryChannel;
    fn last_reading(&self) -> Option<TelemetryFrame>;
}

/// Aggregator that collects telemetry from multiple readers.
pub struct TelemetryBus<const N: usize> {
    frames: [TelemetryFrame; N],
    count: usize,
}

impl<const N: usize> TelemetryBus<N> {
    pub const fn new() -> Self {
        Self {
            frames: [TelemetryFrame {
                channel: TelemetryChannel::Voltage,
                timestamp_us: 0,
                value_i32: 0,
                value_u32: 0,
                flags: 0,
            }; N],
            count: 0,
        }
    }

    pub fn push(&mut self, frame: TelemetryFrame) {
        if self.count < N {
            self.frames[self.count] = frame;
            self.count += 1;
        }
    }

    pub fn drain(&mut self) -> &[TelemetryFrame] {
        let slice = &self.frames[..self.count];
        self.count = 0;
        slice
    }

    pub fn is_full(&self) -> bool {
        self.count >= N
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_status_returns_correct_string() {
        assert_eq!(HealthStatus::Ready.as_str(), "READY\n");
    }

    #[test]
    fn fail_adc_returns_correct_string() {
        let status = HealthStatus::Fail(HealthError::AdcCalibration);
        assert_eq!(status.as_str(), "FAIL:adc\n");
    }

    #[test]
    fn fail_clock_returns_correct_string() {
        let status = HealthStatus::Fail(HealthError::ClockOutOfRange);
        assert_eq!(status.as_str(), "FAIL:clk\n");
    }

    #[test]
    fn sample_packet_basic_roundtrip() {
        let mut pkt = SamplePacket::new();
        assert!(!pkt.is_full());
        assert_eq!(pkt.count, 0);

        pkt.push(ChannelId::Analog0, 100, 2048);
        pkt.push(ChannelId::Analog1, 200, 1024);
        assert_eq!(pkt.count, 2);

        pkt.finalize(1);
        assert_eq!(pkt.magic, SamplePacket::MAGIC);
        assert_eq!(pkt.seq, 1);
    }

    #[test]
    fn sample_packet_fills_up() {
        let mut pkt = SamplePacket::new();
        for i in 0..SAMPLES_PER_PACKET {
            assert!(!pkt.is_full());
            pkt.push(ChannelId::Analog0, i as u32 * 100, i as u32);
        }
        assert!(pkt.is_full());
        assert_eq!(pkt.count as usize, SAMPLES_PER_PACKET);
    }

    #[test]
    fn sample_packet_does_not_overflow() {
        let mut pkt = SamplePacket::new();
        for _ in 0..SAMPLES_PER_PACKET + 10 {
            pkt.push(ChannelId::Analog0, 0, 0);
        }
        assert_eq!(pkt.count as usize, SAMPLES_PER_PACKET);
    }

    #[test]
    fn calibration_counts_to_mv() {
        let cal = AdcCalibration::default();
        let mv = cal.counts_to_mv(2048, ChannelId::Analog0);
        assert_eq!(mv, 1650);
    }

    #[test]
    fn calibration_with_offset() {
        let cal = AdcCalibration {
            vref_mv: 3300,
            offset_ch0: 100,
            offset_ch1: 0,
            gain_ch0: 4095,
            gain_ch1: 4095,
        };
        let mv = cal.counts_to_mv(100, ChannelId::Analog0);
        assert_eq!(mv, 0);
    }

    #[test]
    fn health_error_new_variant() {
        let err = HealthStatus::Fail(HealthError::ClockHclkNotRunning);
        assert_eq!(err.as_str(), "FAIL:hclk\n");
    }

    // ── Fault injection tests ───────────────────────────────────────────

    #[test]
    fn fault_config_new() {
        let cfg = FaultConfig::new(Protocol::Spi, FaultType::BitFlip);
        assert_eq!(cfg.protocol, Protocol::Spi);
        assert_eq!(cfg.fault_type, FaultType::BitFlip);
        assert_eq!(cfg.target_bit, 0);
        assert_eq!(cfg.duration_us, 0);
        assert_eq!(cfg.repeat_count, 1);
        assert_eq!(cfg.probability_permille, 1000);
    }

    #[test]
    fn fault_config_builder() {
        let cfg = FaultConfig::new(Protocol::I2c, FaultType::NackInjection)
            .at_bit(3)
            .for_us(100)
            .repeat(5)
            .probability(500);
        assert_eq!(cfg.protocol, Protocol::I2c);
        assert_eq!(cfg.fault_type, FaultType::NackInjection);
        assert_eq!(cfg.target_bit, 3);
        assert_eq!(cfg.duration_us, 100);
        assert_eq!(cfg.repeat_count, 5);
        assert_eq!(cfg.probability_permille, 500);
    }

    #[test]
    fn fault_type_repr() {
        assert_eq!(FaultType::BitFlip as u8, 0x01);
        assert_eq!(FaultType::CrcCorrupt as u8, 0x09);
        assert_eq!(FaultType::Timeout as u8, 0x0C);
    }

    #[test]
    fn protocol_repr() {
        assert_eq!(Protocol::Spi as u8, 0);
        assert_eq!(Protocol::OneWire as u8, 4);
    }

    #[test]
    fn fault_command_repr() {
        assert_eq!(FaultCommand::Arm as u8, 0x01);
        assert_eq!(FaultCommand::Reset as u8, 0x05);
    }

    #[test]
    fn fault_result_repr() {
        assert_eq!(FaultResult::Armed as u8, 0x01);
        assert_eq!(FaultResult::Completed as u8, 0x06);
    }

    // ── Telemetry tests ─────────────────────────────────────────────────

    #[test]
    fn telemetry_bus_push_and_drain() {
        let mut bus = TelemetryBus::<4>::new();
        assert!(!bus.is_full());

        bus.push(TelemetryFrame {
            channel: TelemetryChannel::Voltage,
            timestamp_us: 100,
            value_i32: 3300,
            value_u32: 3300,
            flags: 0,
        });
        bus.push(TelemetryFrame {
            channel: TelemetryChannel::Temperature,
            timestamp_us: 200,
            value_i32: 25,
            value_u32: 25,
            flags: 0,
        });

        let frames = bus.drain();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].channel, TelemetryChannel::Voltage);
        assert_eq!(frames[1].channel, TelemetryChannel::Temperature);
        assert_eq!(bus.drain().len(), 0);
    }

    #[test]
    fn telemetry_bus_overflow() {
        let mut bus = TelemetryBus::<2>::new();
        for i in 0..5u32 {
            bus.push(TelemetryFrame {
                channel: TelemetryChannel::Current,
                timestamp_us: i,
                value_i32: i as i32,
                value_u32: i,
                flags: 0,
            });
        }
        assert!(bus.is_full());
        assert_eq!(bus.drain().len(), 2);
    }

    // ── Mock FaultInjector test ──────────────────────────────────────────

    struct MockBus;

    struct MockInjector {
        armed: bool,
        count: u32,
    }

    impl MockInjector {
        fn new() -> Self {
            Self { armed: false, count: 0 }
        }
    }

    impl<'d> FaultInjector<'d, MockBus> for MockInjector {
        type Error = ();

        fn configure(&mut self, _config: &FaultConfig) -> Result<(), ()> {
            Ok(())
        }
        fn arm(&mut self) -> Result<(), ()> {
            self.armed = true;
            Ok(())
        }
        fn disarm(&mut self) -> Result<(), ()> {
            self.armed = false;
            Ok(())
        }
        fn fire(&mut self, _bus: &mut MockBus) -> Result<FaultResult, ()> {
            if !self.armed {
                return Err(());
            }
            self.count += 1;
            Ok(FaultResult::Fired)
        }
        fn is_armed(&self) -> bool {
            self.armed
        }
        fn injected_count(&self) -> u32 {
            self.count
        }
        fn reset_stats(&mut self) {
            self.count = 0;
        }
    }

    #[test]
    fn trait_arm_fire_cycle() {
        let mut inj = MockInjector::new();
        let mut bus = MockBus;

        assert!(!inj.is_armed());
        inj.arm().unwrap();
        assert!(inj.is_armed());

        let r = inj.fire(&mut bus).unwrap();
        assert_eq!(r, FaultResult::Fired);
        assert_eq!(inj.injected_count(), 1);

        inj.disarm().unwrap();
        assert!(!inj.is_armed());
        assert!(inj.fire(&mut bus).is_err());
    }

    #[test]
    fn trait_generic_dispatch() {
        fn dispatch<I: FaultInjector<'static, MockBus>>(inj: &mut I, bus: &mut MockBus) {
            inj.arm().unwrap();
            inj.fire(bus).unwrap();
        }

        let mut inj = MockInjector::new();
        let mut bus = MockBus;
        dispatch(&mut inj, &mut bus);
        assert_eq!(inj.injected_count(), 1);
    }
}
