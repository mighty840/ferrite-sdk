MEMORY {
  /* STM32H563ZI */
  FLASH : ORIGIN = 0x08000000, LENGTH = 2048K
  RAM   : ORIGIN = 0x20000000, LENGTH = 639K
  RETAINED (rwx) : ORIGIN = 0x2009FF00, LENGTH = 0x100
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
