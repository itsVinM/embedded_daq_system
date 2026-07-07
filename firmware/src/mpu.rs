/// MPU register block base address (Cortex-M4 TRM §4.5)
const MPU_BASE: u32 = 0xE000_ED90;

// Register offsets from MPU_BASE
const MPU_CTRL: u32  = MPU_BASE + 0x04; // Control
const MPU_RNR:  u32  = MPU_BASE + 0x08; // Region Number
const MPU_RBAR: u32  = MPU_BASE + 0x0C; // Region Base Address
const MPU_RASR: u32  = MPU_BASE + 0x10; // Region Attribute and Size

// MPU_CTRL bits
const CTRL_ENABLE:      u32 = 1 << 0;
const CTRL_PRIVDEFENA:  u32 = 1 << 2; // privileged code falls back to default map

// MPU_RASR bits
const RASR_ENABLE:      u32 = 1 << 0;
const RASR_XN:          u32 = 1 << 28; // execute-never

// Access permission field [26:24] in RASR
const AP_NO_ACCESS:     u32 = 0b000 << 24;
const AP_FULL:          u32 = 0b011 << 24; // RW privileged + unprivileged
const AP_RO:            u32 = 0b110 << 24; // RO privileged + unprivileged

// Memory type bits in RASR [17:16] (TEX=0 implied)
const MEM_NORMAL_CACHED: u32 = 1 << 17;            // C=1 B=0: normal, cacheable
const MEM_DEVICE:        u32 = 1 << 16;            // C=0 B=1: device, strongly ordered

// SIZE field [5:1]: region size = 2^(SIZE+1)
// Only power-of-2 sizes, minimum 32 bytes (SIZE=4)
const fn size_field(size_exp: u32) -> u32 {
    (size_exp - 1) << 1
}

const SIZE_256B:  u32 = size_field(8);   // 2^8  = 256 B
const SIZE_128KB: u32 = size_field(17);  // 2^17 = 128 KB (covers 96KB SRAM)
const SIZE_512KB: u32 = size_field(19);  // 2^19 = 512 KB (flash)
const SIZE_512MB: u32 = size_field(29);  // 2^29 = 512 MB (peripheral space)

/// Configure 4 MPU regions and enable the MPU.
///
/// Must be called before any task is spawned, in a single-threaded context.
///
/// Region priority: higher region number wins on overlap.
/// - Region 0: Flash       — read-only, executable
/// - Region 1: SRAM        — read/write, no execute (XN)
/// - Region 2: Peripherals — device memory, no execute
/// - Region 3: Stack guard — no access; stack overflow → MemFault
pub fn init() {
    unsafe {
        // Disable MPU before touching region config — required by ARM spec
        write_reg(MPU_CTRL, 0);

        // Region 0 — Flash (0x0800_0000, 512 KB)
        // Read-only + executable. Prevents accidental writes to flash via MPU.
        configure_region(
            0,
            0x0800_0000,
            SIZE_512KB,
            AP_RO,
            0, // XN=0: code lives here
            MEM_NORMAL_CACHED,
        );

        // Region 1 — SRAM (0x2000_0000, 128 KB rounded up from 96 KB)
        // Full access, no execute. XN stops ROP-style attacks and catches
        // function pointers that accidentally point into RAM.
        configure_region(
            1,
            0x2000_0000,
            SIZE_128KB,
            AP_FULL,
            RASR_XN,
            MEM_NORMAL_CACHED,
        );

        // Region 2 — Peripheral space (0x4000_0000, 512 MB)
        // Device memory: non-cacheable, non-bufferable, strongly ordered.
        // Covers APB1, APB2, AHB1, AHB2 on STM32F4.
        configure_region(
            2,
            0x4000_0000,
            SIZE_512MB,
            AP_FULL,
            RASR_XN,
            MEM_DEVICE,
        );

        // Region 3 — Stack overflow guard (0x2001_6000, 256 B)
        // Placed 256 B below the stack top (0x2001_8000). Stack grows downward;
        // if it crosses this guard, the MPU raises MemFault immediately.
        configure_region(
            3,
            0x2001_6000,
            SIZE_256B,
            AP_NO_ACCESS,
            RASR_XN,
            MEM_NORMAL_CACHED,
        );

        // Re-enable MPU.
        // PRIVDEFENA: privileged code can still reach addresses not covered
        // by any region (e.g. vendor ROM, SCS). Without it, touching an
        // unconfigured address in privileged mode would also fault.
        write_reg(MPU_CTRL, CTRL_ENABLE | CTRL_PRIVDEFENA);

        // Instruction + data barrier: MPU config must be visible before the
        // first memory access that it should protect.
        core::arch::asm!("dsb", "isb", options(nostack));
    }
}

/// Select region `n`, write its base address and attributes.
///
/// `base` must be naturally aligned to the region size.
/// `size` is one of the SIZE_* constants (SIZE field value, not bytes).
/// `ap` is one of the AP_* constants (access permission bits in place).
/// `xn` is either `RASR_XN` or `0`.
/// `mem` is one of the MEM_* constants.
unsafe fn configure_region(
    region:  u32,
    base:    u32,
    size:    u32,
    ap:      u32,
    xn:      u32,
    mem:     u32,
) {
    write_reg(MPU_RNR,  region);
    write_reg(MPU_RBAR, base);
    write_reg(MPU_RASR, xn | ap | mem | size | RASR_ENABLE);
}

#[inline(always)]
unsafe fn write_reg(addr: u32, val: u32) {
    core::ptr::write_volatile(addr as *mut u32, val);
}
