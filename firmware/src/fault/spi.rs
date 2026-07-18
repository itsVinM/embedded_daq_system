use shared::{FaultConfig, FaultType, FaultResult, FaultInjector};
use super::{bit_flip, bit_set, bit_clear, busy_delay_us, Lfsr, should_inject};

pub struct SpiBus {
    pub sck: bool,
    pub mosi: bool,
    pub miso: bool,
    pub cs: bool,
}

pub struct SpiFaultInjector<'d> {
    config: FaultConfig,
    lfsr: Lfsr,
    armed: bool,
    count: u32,
    _lifetime: core::marker::PhantomData<&'d ()>,
}

impl<'d> SpiFaultInjector<'d> {
    pub fn new() -> Self {
        Self {
            config: FaultConfig::new(shared::Protocol::Spi, FaultType::BitFlip),
            lfsr: Lfsr::new(0xBEEF),
            armed: false,
            count: 0,
            _lifetime: core::marker::PhantomData,
        }
    }

    pub fn inject_mosi(&mut self, byte: u8) -> u8 {
        if !self.armed || !should_inject(&mut self.lfsr, self.config.probability_permille) {
            return byte;
        }
        let bit = if self.config.target_bit < 8 {
            self.config.target_bit
        } else {
            self.lfsr.next_bit()
        };
        let result = match self.config.fault_type {
            FaultType::BitFlip => bit_flip(byte, bit),
            FaultType::StuckAtZero => bit_clear(byte, bit),
            FaultType::StuckAtOne => bit_set(byte, bit),
            FaultType::BitDelay => { busy_delay_us(self.config.duration_us); byte }
            FaultType::ClockGlitch => { busy_delay_us(5); bit_flip(byte, self.lfsr.next_bit()) }
            _ => byte,
        };
        if result != byte { self.count += 1; }
        result
    }

    pub fn inject_miso(&mut self, byte: u8) -> u8 {
        if !self.armed || !should_inject(&mut self.lfsr, self.config.probability_permille) {
            return byte;
        }
        let bit = if self.config.target_bit < 8 {
            self.config.target_bit
        } else {
            self.lfsr.next_bit()
        };
        let result = match self.config.fault_type {
            FaultType::BitFlip | FaultType::StuckAtZero | FaultType::StuckAtOne => {
                match self.config.fault_type {
                    FaultType::BitFlip => bit_flip(byte, bit),
                    FaultType::StuckAtZero => bit_clear(byte, bit),
                    _ => bit_set(byte, bit),
                }
            }
            _ => byte,
        };
        if result != byte { self.count += 1; }
        result
    }
}

impl<'d, 'b> FaultInjector<'d, SpiBus> for SpiFaultInjector<'d> {
    type Error = ();

    fn configure(&mut self, config: &FaultConfig) -> Result<(), ()> {
        self.config = *config;
        Ok(())
    }

    fn arm(&mut self) -> Result<(), ()> {
        self.armed = true;
        self.count = 0;
        Ok(())
    }

    fn disarm(&mut self) -> Result<(), ()> {
        self.armed = false;
        Ok(())
    }

    fn fire(&mut self, bus: &mut SpiBus) -> Result<FaultResult, ()> {
        if !self.armed { return Err(()); }
        let _ = self.inject_mosi(bus.mosi as u8);
        let _ = self.inject_miso(bus.miso as u8);
        Ok(FaultResult::Fired)
    }

    fn is_armed(&self) -> bool { self.armed }
    fn injected_count(&self) -> u32 { self.count }
    fn reset_stats(&mut self) { self.count = 0; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spi_bit_flip() {
        let cfg = FaultConfig::new(shared::Protocol::Spi, FaultType::BitFlip).at_bit(0);
        let mut inj = SpiFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert_eq!(inj.inject_mosi(0b1010_0000), 0b1010_0001);
    }

    #[test]
    fn spi_stuck_at_zero() {
        let cfg = FaultConfig::new(shared::Protocol::Spi, FaultType::StuckAtZero).at_bit(7);
        let mut inj = SpiFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert_eq!(inj.inject_mosi(0xFF), 0x7F);
    }

    #[test]
    fn spi_stuck_at_one() {
        let cfg = FaultConfig::new(shared::Protocol::Spi, FaultType::StuckAtOne).at_bit(3);
        let mut inj = SpiFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert_eq!(inj.inject_mosi(0x00), 0x08);
    }

    #[test]
    fn spi_no_inject_when_disarmed() {
        let mut inj = SpiFaultInjector::new();
        assert_eq!(inj.inject_mosi(0xAA), 0xAA);
        assert_eq!(inj.injected_count(), 0);
    }

    #[test]
    fn spi_trait_fire() {
        let cfg = FaultConfig::new(shared::Protocol::Spi, FaultType::BitFlip).at_bit(0);
        let mut inj = SpiFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        let mut bus = SpiBus { sck: true, mosi: true, miso: false, cs: true };
        assert_eq!(inj.fire(&mut bus).unwrap(), FaultResult::Fired);
        assert!(inj.is_armed());
    }
}
