/* Nucleo-H563ZI flash layout shared by bootloader and app (8 KB sectors).
 * Keep in sync with ../embassy-stm32h563-bootloader/memory.x.
 *
 *   0x08000000  48 KB  bootloader
 *   0x0800C000   8 KB  bootloader state
 *   0x08010000 256 KB  ACTIVE: this app (linked here)
 *   0x08100000 264 KB  DFU: update slot in bank 2
 */
MEMORY
{
  BOOTLOADER       : ORIGIN = 0x08000000, LENGTH = 48K
  BOOTLOADER_STATE : ORIGIN = 0x0800C000, LENGTH = 8K
  FLASH            : ORIGIN = 0x08010000, LENGTH = 256K
  DFU              : ORIGIN = 0x08100000, LENGTH = 264K
  RAM              : ORIGIN = 0x20000000, LENGTH = 640K
}

/* embassy-boot partition offsets, relative to the start of flash */
__bootloader_state_start = ORIGIN(BOOTLOADER_STATE) - ORIGIN(BOOTLOADER);
__bootloader_state_end   = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE) - ORIGIN(BOOTLOADER);
__bootloader_dfu_start   = ORIGIN(DFU) - ORIGIN(BOOTLOADER);
__bootloader_dfu_end     = ORIGIN(DFU) + LENGTH(DFU) - ORIGIN(BOOTLOADER);

SECTIONS {
  .uninit.ferrite (NOLOAD) : {
    . = ALIGN(4);
    _ferrite_retained_start = .;
    KEEP(*(.uninit.ferrite))
    _ferrite_retained_end = .;
    . = ALIGN(4);
  } > RAM
}
