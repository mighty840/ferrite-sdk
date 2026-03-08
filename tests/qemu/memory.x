MEMORY {
  FLASH (rx)    : ORIGIN = 0x00000000, LENGTH = 256K
  RAM   (rwx)   : ORIGIN = 0x20000000, LENGTH = 63K
  RETAINED (rw) : ORIGIN = 0x2000FC00, LENGTH = 1K
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
