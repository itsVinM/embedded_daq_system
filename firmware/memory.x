/* Application sits after a 32 KB bootloader (sectors 0-1 of STM32F401RE flash).
 * cortex-m-rt places the vector table at FLASH ORIGIN, so VTOR will be 0x0800_8000.
 * The bootloader reads this address before jumping here.
 *
 * To run firmware WITHOUT the bootloader (dev/debug): change ORIGIN back to
 * 0x08000000 and LENGTH to 512K.
 */
MEMORY
{
  FLASH : ORIGIN = 0x08008000, LENGTH = 480K
  RAM   : ORIGIN = 0x20000000, LENGTH = 96K
}

SECTIONS
{
  .sram_test (NOLOAD) : ALIGN(4)
  {
    _sram_test_start = .;
    . = . + 64;
    _sram_test_end = .;
  } > RAM
}
