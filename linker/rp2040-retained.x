/* ferrite-sdk retained RAM section for RP2040 */
/* RP2040: 264KB SRAM, use end of main SRAM region */

MEMORY {
  /* ... your existing MEMORY block ... */
  RETAINED (rwx) : ORIGIN = 0x20041F00, LENGTH = 0x100
}

SECTIONS {
  .uninit.ferrite (NOLOAD) : {
    . = ALIGN(4);
    _ferrite_retained_start = .;
    KEEP(*(.uninit.ferrite))
    _ferrite_retained_end = .;
    . = ALIGN(4);
  } > RETAINED
}
