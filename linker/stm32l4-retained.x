/* ferrite-sdk retained RAM section for STM32L4 */
/* STM32L4A6/L496: 320KB SRAM (SRAM1 256KB + SRAM2 64KB), use end of SRAM */

MEMORY {
  /* ... your existing MEMORY block ... */
  RETAINED (rwx) : ORIGIN = 0x2004FC00, LENGTH = 0x400
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
