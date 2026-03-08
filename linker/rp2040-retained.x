/* iotai-sdk retained RAM section for RP2040 */
/* RP2040: 264KB SRAM, use end of main SRAM region */

MEMORY {
  /* ... your existing MEMORY block ... */
  RETAINED (rwx) : ORIGIN = 0x20041F00, LENGTH = 0x100
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
