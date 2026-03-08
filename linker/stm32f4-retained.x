/* iotai-sdk retained RAM section for STM32F4 */
/* STM32F4xx: 128KB SRAM, use end of SRAM1 */

MEMORY {
  /* ... your existing MEMORY block ... */
  RETAINED (rwx) : ORIGIN = 0x2001FF00, LENGTH = 0x100
}

SECTIONS {
  .uninit.iotai (NOLOAD) : {
    . = ALIGN(4);
    _iotai_retained_start = .;
    KEEP(*(.uninit.iotai))
    _iotai_retained_end = .;
    . = ALIGN(4);
  } > RETAINED
}
