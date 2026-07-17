use defmt::info;
use embassy_stm32::peripherals::USART2;
use embassy_stm32::Peri;
use embassy_stm32::usart::{Config, Uart};
use embassy_stm32::mode::Async;
use embassy_stm32::bind_interrupts;

use shared::{SamplePacket, AcquisitionConfig};

bind_interrupts!(struct Irqs {
    USART2 => embassy_stm32::usart::InterruptHandler<USART2>;
});

pub struct Transport {
    uart: Uart<'static, Async>,
    enabled: bool,
}

impl Transport {
    /// Create a new USART2 transport on PA2 (TX) and PA3 (RX) at 115200 baud.
    pub fn new(
        usart: Peri<'static, USART2>,
        tx: Peri<'static, embassy_stm32::peripherals::PA2>,
        rx: Peri<'static, embassy_stm32::peripherals::PA3>,
    ) -> Self {
        let config = Config::new(115_200);
        let uart = Uart::new(usart, tx, rx, config, Irqs);
        Self { uart, enabled: false }
    }

    /// Enable or disable the transport.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Send a populated sample packet to the host.
    /// Only sends if transport is enabled.
    pub async fn send_packet(&mut self, pkt: &SamplePacket) {
        if !self.enabled {
            return;
        }
        let _ = self.uart.write(pkt.as_bytes()).await;
    }

    /// Send the current acquisition configuration to the host.
    pub async fn send_config(&mut self, cfg: &AcquisitionConfig) {
        let bytes = unsafe {
            core::slice::from_raw_parts(
                cfg as *const _ as *const u8,
                core::mem::size_of::<AcquisitionConfig>(),
            )
        };
        let _ = self.uart.write(bytes).await;
    }
}