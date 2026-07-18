# Embedded DAQ System

Bare-metal mixed-signal analyser for STM32F401RE (Nucleo-64). Rust, Embassy async, no heap.

## Features

| Module | Description |
|--------|-------------|
| Analog | ADC1 DMA ring buffer — dual-channel interleaved, 10 kHz |
| Digital | TIM1 PWM output (4ch), TIM3 input capture |
| Transport | USART2 binary packet stream (115200 baud) |
| Fault Injection | SPI, I2C, UART, CAN, OneWire fault injection via UART command |

## Architecture

- `FaultInjector<'d, B>` trait — generic over bus type `B`, lifetime `'d`
- Feature flags: `analog` (default), `digital`, `fault`
- Zero heap, MPU (flash RO, SRAM RW+XN, stack guard)

## Pin mapping

```
PA0  ADC1 CH0        PA6  TIM3 CH1
PA1  ADC1 CH1        PA8  TIM1 CH1 (25%)
PA2  USART2 TX       PA9  TIM1 CH2 (50%)
PA3  USART2 RX       PA10 TIM1 CH3 (75%)
PB6  Fault UART TX   PA11 TIM1 CH4 (10%)
PB7  Fault UART RX
```

## Build

```bash
cargo build --release                                          # analog
cargo build --release --no-default-features --features digital # digital
cargo build --release --features fault                         # + fault injection
```

## Flash

```bash
probe-rs run --chip STM32F401RETx target/thumbv7em-none-eabihf/release/firmware
```

## Tests

```bash
cd shared && cargo test
```

## Docker

```bash
docker compose up --build
```
