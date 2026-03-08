/* ferrite-sdk retained RAM section for nRF52840 */
/* Add this BEFORE your SECTIONS in memory.x */
/* Retained RAM: not cleared on soft reset */
/* nRF52840: use end of RAM block 1 */

MEMORY {
  /* ... your existing MEMORY block ... */
  RETAINED (rwx) : ORIGIN = 0x20003F00, LENGTH = 0x100
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
