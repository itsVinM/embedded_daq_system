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
    fn sample_packet_byte_size() {
        let pkt = SamplePacket::new();
        let expected = core::mem::size_of::<u16>() * 3 + core::mem::size_of::<Sample>() * SAMPLES_PER_PACKET;
        assert_eq!(pkt.as_bytes().len(), expected);
    }

    #[test]
    fn health_error_new_variant() {
        let err = HealthStatus::Fail(HealthError::ClockHclkNotRunning);
        assert_eq!(err.as_str(), "FAIL:hclk\n");
    }
}
