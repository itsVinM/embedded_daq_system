use defmt::info;
use embassy_stm32::{
    adc::{Adc, AdcChannel, AnyAdcChannel, SampleTime},
    bind_interrupts, dma, peripherals,
    Peri,
};
use shared::DmaBuf;

bind_interrupts!(struct Irqs {
    DMA2_STREAM0 => dma::InterruptHandler<peripherals::DMA2_CH0>;
});

static mut ADC_BUF: DmaBuf<u16, 4096> = DmaBuf::new();


pub struct AnalogSampler {
    pa0_mv: u32,
    pa1_mv: u32,
    sample_count: u32,
}

impl AnalogSampler {
    fn new() -> Self {
        Self { 
            pa0_mv : 0,
            pa1_mv : 0,
            sample_count : 0,
        }
    }
    fn counts_to_mv(counts : u32) -> u32 {
        (counts * 3300) / 4095
    }

    fn update_analog(&mut self, measurements: &[u16], count: usize) {
        if count < 2 { return; }  
        
        self.pa0_mv = Self::counts_to_mv(
            measurements[..count].iter().step_by(2)
                .map(|&x| x as u32).sum::<u32>() / (count / 2) as u32
        );
        self.pa1_mv = Self::counts_to_mv(
            measurements[..count].iter().skip(1).step_by(2)
                .map(|&x| x as u32).sum::<u32>() / (count / 2) as u32
        );
        self.sample_count += count as u32;
    }

    fn log(&self){
        info!("PA0={} mV PA1={} mV n={}", 
            self.pa0_mv, 
            self.pa1_mv, 
            self.sample_count);
    }
}

#[embassy_executor::task]
pub async fn adc_task(
    adc: Peri<'static, peripherals::ADC1>,
    dma: Peri<'static, peripherals::DMA2_CH0>,
    mut pin0: Peri<'static, peripherals::PA0>,
    mut pin1: Peri<'static, peripherals::PA1>,
) {
    let mut adc_driver = Adc::new(adc);
    let channel0: AnyAdcChannel<peripherals::ADC1> = pin0.degrade_adc();
    let channel1: AnyAdcChannel<peripherals::ADC1> = pin1.degrade_adc();
    
    let buf = unsafe { ADC_BUF.as_mut_slice() };

    let mut ring = adc_driver.into_ring_buffered(
        dma,
        buf,
        Irqs,
        [
            (channel0, SampleTime::Cycles15),
            (channel1, SampleTime::Cycles15),
        ].into_iter(),
        None,
    );

    let mut adc_sampler = AnalogSampler::new();
    let mut measurements = [0u16; 2048];
    
    loop {
        let Ok(n) = ring.read(&mut measurements).await else { 
            continue 
        };
        let count = n.min(measurements.len());
        if count == 0 { continue; }
        
        adc_sampler.update_analog(&measurements, count);
        adc_sampler.log();
        
    }
}

