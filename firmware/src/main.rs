#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::rcc::{
    APBPrescaler, Pll, PllMul, PllPDiv, PllPreDiv, PllSource, Sysclk,
};
use embassy_stm32::Peri;
use embassy_stm32::peripherals::RCC;
use embassy_stm32::Config;
use panic_probe as _;
use shared::{HealthStatus, AcquisitionConfig};

mod health;
mod mpu;
mod analog;
mod digital;
mod transport;

#[embassy_executor::main]
async fn main(spawner: Spawner) {

    // MPU must be configured before any peripheral or memory access.
    // Regions: flash RO, SRAM RW+XN, peripherals device, stack guard no-access.
    mpu::init();
    info!("mpu configured");

    // ==== CLOCK TREE CONFIG ======
    let mut config = Config::default();
    config.rcc.pll = Some(Pll {
        prediv: PllPreDiv::Div8,   // M=8
        mul: PllMul::Mul168,       // N=168
        divp: Some(PllPDiv::Div4), // P=4 → 84 MHz
        divq: None,
        divr: None,
    });
    config.rcc.pll_src = PllSource::Hsi;
    config.rcc.sys = Sysclk::Pll1P;
    config.rcc.apb1_pre = APBPrescaler::Div2;  // 84/2 = 42 MHz — within limit
    config.rcc.apb2_pre = APBPrescaler::Div1;  // 84/1 = 84 MHz 

    let peripherals = embassy_stm32::init(config);
    info!("booting...");
    health_check(peripherals.RCC).await;

    // Configure flash ART accelerator: 5 wait states for 84 MHz @ 3.3V
    // STM32F401RE requires 5 WS at 84 MHz per RM0368 §3.5.1
    unsafe {
        let flash = &*embassy_stm32::pac::FLASH::ptr();
        flash.acr().modify(|w| {
            w.set_latency(5);
            w.set_prften(true);
            w.set_icen(true);
            w.set_dcen(true);
        });
    }

    #[cfg(feature = "analog")]
    {
        info!("=== ANALOG MODE ===");
        spawner.spawn(analog::adc_task(
            peripherals.ADC1,
            peripherals.DMA2_CH0,
            peripherals.PA0,
            peripherals.PA1,
        ).unwrap())
    }

    #[cfg(feature = "digital")]
    {
        // PWM OUTPUT: PA8, PA9, PA10, PA11 (TIM1 Ch1-4)
        spawner.spawn(digital::pwm_task(
            peripherals.TIM1,
            peripherals.PA8,   // Ch1 - 25% duty
            peripherals.PA9,   // Ch2 - 50% duty
            peripherals.PA10,  // Ch3 - 75% duty
            peripherals.PA11,  // Ch4 - 10% duty
        ).unwrap());
  
        // INPUT CAPTURE: PA6 (TIM3 Ch1)
        spawner.spawn(digital::capture_task(
            peripherals.TIM3,
            peripherals.PA6,
        ).unwrap());
    }

    #[cfg(all(feature = "analog", feature = "digital"))]
    {
        info!("!!! Both analog and digital enabled - running BOTH !!!");
    }
 
    #[cfg(not(any(feature = "analog", feature = "digital")))]
    {
        info!("WARNING: No features enabled. Enable 'analog' or 'digital' in Cargo.toml");
        loop {}
    }
}


async fn health_check(rcc: Peri<'_, RCC>){
    let clock_status = health::check_clock(rcc);
    if clock_status != HealthStatus::Ready {
        info!("health FAILED: {}", clock_status.as_str());
        loop {}
    }

    let canary_status = health::check_stack_canary();
    if canary_status != HealthStatus::Ready {
        info!("health FAILED: {}", canary_status.as_str());
        loop {}
    }

    let ram_status = health::check_ram();
    if ram_status != HealthStatus::Ready {
        info!("health FAILED: {}", ram_status.as_str());
        loop {}
    }
}