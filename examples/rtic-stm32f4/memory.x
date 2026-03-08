MEMORY {
  FLASH : ORIGIN = 0x08000000, LENGTH = 512K
  RAM   : ORIGIN = 0x20000000, LENGTH = 127K
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
