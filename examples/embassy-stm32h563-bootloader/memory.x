/* Nucleo-H563ZI flash layout shared by bootloader and app (8 KB sectors).
 * Keep in sync with ../embassy-stm32h563/memory.x.
 *
 *   0x08000000  48 KB  bootloader (this crate)
 *   0x0800C000   8 KB  bootloader state (swap progress, booted/updated marks)
 *   0x08010000 256 KB  ACTIVE: the running app
 *   0x08100000 264 KB  DFU: update slot in bank 2 (ACTIVE + one sector),
 *                      so the app can write it while running from bank 1
 */
MEMORY
{
  FLASH            : ORIGIN = 0x08000000, LENGTH = 48K
  BOOTLOADER_STATE : ORIGIN = 0x0800C000, LENGTH = 8K
  ACTIVE           : ORIGIN = 0x08010000, LENGTH = 256K
  DFU              : ORIGIN = 0x08100000, LENGTH = 264K
  RAM              : ORIGIN = 0x20000000, LENGTH = 640K
}

/* embassy-boot partition offsets, relative to the start of flash */
__bootloader_state_start  = ORIGIN(BOOTLOADER_STATE) - ORIGIN(FLASH);
__bootloader_state_end    = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE) - ORIGIN(FLASH);
__bootloader_active_start = ORIGIN(ACTIVE) - ORIGIN(FLASH);
__bootloader_active_end   = ORIGIN(ACTIVE) + LENGTH(ACTIVE) - ORIGIN(FLASH);
__bootloader_dfu_start    = ORIGIN(DFU) - ORIGIN(FLASH);
__bootloader_dfu_end      = ORIGIN(DFU) + LENGTH(DFU) - ORIGIN(FLASH);
