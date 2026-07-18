use defmt::info;
use embassy_stm32::peripherals::{USART2, PA2, PA3, DMA1_CH5, DMA1_CH6};
use embassy_stm32::Peri;
use embassy_stm32::usart::{Config, Uart};
use embassy_stm32::mode::Async;
use embassy_stm32::bind_interrupts;

use shared::{SamplePacket, AcquisitionConfig};

bind_interrupts!(struct Irqs {
    USART2 => embassy_stm32::usart::InterruptHandler<USART2>;
    DMA1_STREAM5 => embassy_stm32::dma::InterruptHandler<DMA1_CH5>;
    DMA1_STREAM6 => embassy_stm32::dma::InterruptHandler<DMA1_CH6>;
});

pub struct Transport {
    uart: Uart<'static, Async>,
    enabled: bool,
}

impl Transport {
    pub fn new(
        usart: Peri<'static, USART2>,
        tx: Peri<'static, PA2>,
        rx: Peri<'static, PA3>,
        tx_dma: Peri<'static, DMA1_CH6>,
        rx_dma: Peri<'static, DMA1_CH5>,
    ) -> Self {
        let mut config = Config::default();
        config.baudrate = 115_200;
        let uart = Uart::new(usart, rx, tx, tx_dma, rx_dma, Irqs, config)
            .expect("uart init failed");
        Self { uart, enabled: false }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub async fn send_packet(&mut self, pkt: &SamplePacket) {
        if !self.enabled { return; }
        let _ = self.uart.write(pkt.as_bytes()).await;
    }

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
