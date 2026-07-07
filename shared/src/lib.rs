#![cfg_attr(not(test), no_std)]

use core::mem::MaybeUninit;

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