# stm32-daq

Mixed-signal DAQ and fault injection platform for STM32F401RE (Nucleo-64). Rust, bare-metal, no heap.

## What

Acquisition + fault injection on one board. Analog capture, digital I/O, serial telemetry, and protocol fault injection — all feature-gated, all no-std.

## Modules

| Module | Feature flag | Description |
|--------|-------------|-------------|
| Analog | `analog` (default) | ADC1 DMA ring buffer — dual-channel interleaved, 10 kHz |
| Digital | `digital` | TIM1 PWM output (4ch), TIM3 input capture |
| Transport | always | USART2 binary packet stream (115200 baud) |
| Fault | `fault` | SPI, I2C, UART, CAN, OneWire fault injection |

## Pin map

```
PA0  ADC1 CH0        PA6  TIM3 CH1
PA1  ADC1 CH1        PA8  TIM1 CH1 (25%)
PA2  USART2 TX       PA9  TIM1 CH2 (50%)
PA3  USART2 RX       PA10 TIM1 CH3 (75%)
PB6  Fault UART TX   PA11 TIM1 CH4 (10%)
PB7  Fault UART RX
```

## Architecture

```
firmware/src/main.rs           — entry, peripheral init
firmware/src/analog.rs         — ADC DMA ring buffer
firmware/src/digital.rs        — PWM + input capture
firmware/src/transport.rs      — USART2 binary packets
firmware/src/fault/mod.rs      — FaultEngine, UartBitbang
firmware/src/fault/spi.rs      — SPI injector
firmware/src/fault/i2c.rs      — I2C injector
firmware/src/fault/uart.rs     — UART injector
firmware/src/fault/can.rs      — CAN injector (MCP2515)
firmware/src/fault/onewire.rs  — OneWire injector
shared/src/lib.rs              — FaultInjector<'d, B> trait, tests
```

Key pattern: `FaultInjector<'d, B>` — generic over bus type `B`, lifetime `'d`. Same trait in both this project and `rust_fault_injector`.

## Build

```bash
cargo build --release                                          # analog (default)
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
