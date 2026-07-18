use shared::{FaultConfig, FaultType, FaultResult, FaultInjector};
use super::{bit_flip, bit_set, bit_clear, busy_delay_us, Lfsr, should_inject};

pub struct UartBus {
    pub tx: u8,
    pub rx: u8,
}

pub struct UartFaultInjector<'d> {
    config: FaultConfig,
    lfsr: Lfsr,
    armed: bool,
    count: u32,
    overrun_active: bool,
    _lifetime: core::marker::PhantomData<&'d ()>,
}

impl<'d> UartFaultInjector<'d> {
    pub fn new() -> Self {
        Self {
            config: FaultConfig::new(shared::Protocol::Uart, FaultType::BitFlip),
            lfsr: Lfsr::new(0xDEAD),
            armed: false,
            count: 0,
            overrun_active: false,
            _lifetime: core::marker::PhantomData,
        }
    }

    pub fn inject_tx(&mut self, byte: u8) -> u8 {
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
            FaultType::ParityError => byte ^ 0x80,
            FaultType::FrameCorrupt => bit_flip(byte, self.lfsr.next_bit()),
            FaultType::BitDelay => { busy_delay_us(self.config.duration_us); byte }
            FaultType::ClockGlitch => { busy_delay_us(3); bit_flip(byte, self.lfsr.next_bit()) }
            _ => byte,
        };
        if result != byte { self.count += 1; }
        result
    }

    pub fn inject_rx(&mut self, byte: u8) -> u8 {
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
            _ => byte,
        };
        if result != byte { self.count += 1; }
        result
    }

    pub fn inject_overrun(&mut self) -> bool {
        if self.armed && self.config.fault_type == FaultType::Overrun
            && should_inject(&mut self.lfsr, self.config.probability_permille)
        {
            self.overrun_active = true;
            self.count += 1;
            true
        } else {
            false
        }
    }

    pub fn is_overrun_active(&self) -> bool { self.overrun_active }
    pub fn clear_overrun(&mut self) { self.overrun_active = false; }
}

impl<'d, 'b> FaultInjector<'d, UartBus> for UartFaultInjector<'d> {
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
        self.overrun_active = false;
        Ok(())
    }

    fn fire(&mut self, bus: &mut UartBus) -> Result<FaultResult, ()> {
        if !self.armed { return Err(()); }
        bus.tx = self.inject_tx(bus.tx);
        if self.config.fault_type == FaultType::Overrun {
            self.inject_overrun();
        }
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
    fn uart_bit_flip_tx() {
        let cfg = FaultConfig::new(shared::Protocol::Uart, FaultType::BitFlip).at_bit(0);
        let mut inj = UartFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert_eq!(inj.inject_tx(0b1010_0000), 0b1010_0001);
    }

    #[test]
    fn uart_stuck_at_zero_rx() {
        let cfg = FaultConfig::new(shared::Protocol::Uart, FaultType::StuckAtZero).at_bit(4);
        let mut inj = UartFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert_eq!(inj.inject_rx(0xFF), 0xEF);
    }

    #[test]
    fn uart_overrun() {
        let cfg = FaultConfig::new(shared::Protocol::Uart, FaultType::Overrun);
        let mut inj = UartFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        assert!(inj.inject_overrun());
        assert!(inj.is_overrun_active());
        inj.clear_overrun();
        assert!(!inj.is_overrun_active());
    }

    #[test]
    fn uart_trait_fire() {
        let cfg = FaultConfig::new(shared::Protocol::Uart, FaultType::BitFlip).at_bit(0);
        let mut inj = UartFaultInjector::new();
        inj.configure(&cfg).unwrap();
        inj.arm().unwrap();
        let mut bus = UartBus { tx: 0xAA, rx: 0x55 };
        assert_eq!(inj.fire(&mut bus).unwrap(), FaultResult::Fired);
        assert_eq!(bus.tx, 0xAB);
    }
}
