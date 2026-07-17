# Mixed Signal Analyser — STM32F401RE

Bare-metal firmware for a mixed signal analyser running on the STM32F401RE (Nucleo-64).
Written in Rust using the Embassy async executor — no heap, no OS, no RTOS abstraction layer.

---

## What it does

| Subsystem | Hardware | Detail |
|-----------|----------|--------|
| Analogue capture | PA0, PA1 → ADC1 | DMA ring buffer (4096 samples), interleaved 2-channel, converted to mV |
| PWM generation | PA8–PA11 → TIM1 | 4 channels at 10 kHz: 25 / 50 / 75 / 10 % duty cycles |
| PWM input capture | PA6 → TIM3 | Measures period ticks, pulse width, duty cycle % |
| Boot health checks | RCC, SRAM | Clock tree verification, stack canary (0xDEAD_BEEF), RAM pattern test |

Analogue and digital modes are selected at compile time via feature flags — no branching at runtime.

---

## Technical highlights

- **DMA ring buffer** — ADC samples land in a static [u16; 4096] without CPU intervention; Embassy's into_ring_buffered drives continuous dual-channel acquisition
- **Two independent timers** — TIM1 as SimplePwm (output), TIM3 as PwmInput (input capture), wired via bind_interrupts!
- **Boot integrity** — before spawning any task, the firmware verifies the clock tree, writes/reads a stack canary, and pattern-tests 16 words of SRAM using write_volatile/read_volatile
- **MPU** — 4 hardware memory protection regions configured before init: flash read-only, SRAM no-execute, peripheral space device memory, stack overflow guard at 0x2000_4000
- **Async on Cortex-M4** — Embassy's cooperative scheduler; tasks yield at .await points, no preemption needed
- **Zero heap** — no alloc, no dynamic dispatch; all buffers are static
- **Feature-based mode selection** — compile-time analog/digital mode switching with no runtime branching
- **USART2 transport** — PA2/PA3 for sending SamplePacket data to host

---

## Flash layout

```
0x0800_8000  firmware    (480 KB — sectors 2–7)
0x2000_0000  SRAM        (96 KB)
0x2000_4000    └── stack overflow guard (256 B, MPU region 3 → MemFault)
0x2001_8000    └── stack top
```

## Build & flash

```bash
# Build and flash firmware (analog mode)
cd firmware && cargo build --release
probe-rs download --chip STM32F401RETx target/thumbv7em-none-eabihf/release/firmware

# Attach RTT and run
probe-rs run --chip STM32F401RETx target/thumbv7em-none-eabihf/release/firmware

# Digital mode
cargo build --release --no-default-features --features digital
```

Requires [probe-rs](https://probe.rs) and an ST-Link on the Nucleo board.

---

## Host Software

A Python client is included to receive data from the firmware over USART2:

```bash
cd host && python daq_client.py
```

### Host Protocol
- Baud rate: 115200
- Frame format: SamplePacket binary (fixed 2.5 KB)
  - Magic: 2 bytes (0xDA71)
  - Sequence number: 2 bytes
  - Sample count: 2 bytes
  - 32 samples × (1 + 1 + 4 + 4) = 1280 bytes
- Commands: Start stream, stop stream, query config (via shell interface)

The Python client automatically detects the connection and logs ADC measurements in real time.

---

## Hardware

**Board:** NUCLEO-F401RE  
**MCU:** STM32F401RE — Cortex-M4 @ 84 MHz (configured via PLL: HSI → /8 → ×168 → /4)

```
PA0  — ADC1 CH0  (analog in)
PA1  — ADC1 CH1  (analog in)
PA2  — USART2 TX (host communication)
PA3  — USART2 RX (host communication)
PA6  — TIM3 CH1  (PWM capture in)
PA8  — TIM1 CH1  (PWM out 25%)
PA9  — TIM1 CH2  (PWM out 50%)
PA10 — TIM1 CH3  (PWM out 75%)
PA11 — TIM1 CH4  (PWM out 10%)
```


## Getting Started

### Prerequisites
- [Rust](https://rust-lang.org/) with nightly toolchain (for async features)
- [cargo-binutils](https://github.com/rust-embedded/cargo-binutils) for flashing
- [probe-rs](https://probe.rs) for flashing and debugging
- ST-Link V2 programmer/mini programmer
- Python 3.6+ (for host client)

### Quick Start

1. **Clone the repository**
```bash
git clone /path/to/embedded_daq_system
```

2. **Build and flash analog mode firmware**
```bash
cd firmware
cargo build --release
probe-rs download --chip STM32F401RETx target/thumbv7em-none-eabihf/release/firmware
cd ..
```

3. **Run host client**
```bash
cd host
python daq_client.py COM3
```
(Replace COM3 with your ST-Link port)

### Build Variants

#### Analog Mode
```bash
cargo build --release
cargo run --release --features analog
```

#### Digital Mode
```bash
cargo build --release --no-default-features --features digital
cargo run --release --no-default-features --features digital
```



