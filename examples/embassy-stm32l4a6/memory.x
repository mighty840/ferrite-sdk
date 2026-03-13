MEMORY {
  /* STM32L4A6ZG */
  FLASH : ORIGIN = 0x08000000, LENGTH = 1024K
  RAM   : ORIGIN = 0x20000000, LENGTH = 319K
  RETAINED (rwx) : ORIGIN = 0x2004FF00, LENGTH = 0x100
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
