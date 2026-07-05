#![cfg_attr(not(test), no_std)]

use core::mem::MaybeUninit;

/// A statically-allocated DMA-safe buffer.
///
/// Uses `MaybeUninit<T>` to correctly model that the DMA controller
/// (not the CPU) initializes the buffer contents. This avoids the
/// unnecessary zero-initialization cost of `static mut [T; N]`.
#[repr(C)]
pub struct DmaBuf<T, const N: usize> {
    buf: [MaybeUninit<T>; N],
}

impl<T, const N: usize> DmaBuf<T, N> {
    /// Create an uninitialized DMA buffer with no CPU-side initialization cost.
    pub const fn new() -> Self {
        Self { buf: unsafe { MaybeUninit::uninit().assume_init() } }
    }

    /// Returns the buffer length.
    pub const fn len(&self) -> usize {
        N
    }

    /// Returns a raw mutable pointer for the DMA controller to write into.
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.buf.as_mut_ptr() as *mut T
    }

    /// Unsafely assume the buffer is initialized and return it as a mutable slice.
    ///
    /// # Safety
    /// The caller must ensure the DMA controller has fully written the buffer
    /// and that no DMA transfer is currently in flight.
    pub unsafe fn as_mut_slice(&mut self) -> &mut [T] {
        core::slice::from_raw_parts_mut(self.buf.as_mut_ptr() as *mut T, N)
    }
}

#[derive(Debug, PartialEq)]
pub enum HealthStatus {
    Ready,
    Fail(HealthError),
}

#[derive(Debug, PartialEq)]
pub enum HealthError {
    // Memory
    StackCanary,
    RamTest,
    FlashCRC,
    // Clocks
    TimerNotTicking,
    ClockOutOfRange,
    // Peripherals
    AdcCalibration,
    I2cInitFailed,
    SpiInitFailed,
    UartInitFailed,
    DmaInitFailed,
}

impl HealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthStatus::Ready                              => "READY\n",
            HealthStatus::Fail(HealthError::AdcCalibration) => "FAIL:adc\n",
            HealthStatus::Fail(HealthError::TimerNotTicking) => "FAIL:tim\n",
            HealthStatus::Fail(HealthError::ClockOutOfRange) => "FAIL:clk\n",
            HealthStatus::Fail(HealthError::StackCanary)    => "FAIL:stack\n",
            HealthStatus::Fail(HealthError::RamTest)        => "FAIL:ram\n",
            HealthStatus::Fail(HealthError::FlashCRC)       => "FAIL:flash\n",
            HealthStatus::Fail(HealthError::I2cInitFailed)  => "FAIL:i2c\n",
            HealthStatus::Fail(HealthError::SpiInitFailed)  => "FAIL:spi\n",
            HealthStatus::Fail(HealthError::UartInitFailed) => "FAIL:uart\n",
            HealthStatus::Fail(HealthError::DmaInitFailed)  => "FAIL:dma\n",
        }
    }
}

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
}