MEMORY {
  FLASH : ORIGIN = 0x00000000, LENGTH = 1024K
  RAM   : ORIGIN = 0x20000000, LENGTH = 255K
  RETAINED (rwx) : ORIGIN = 0x2003FF00, LENGTH = 0x100
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
