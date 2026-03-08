MEMORY {
  BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
  FLASH : ORIGIN = 0x10000100, LENGTH = 2048K - 0x100
  RAM   : ORIGIN = 0x20000000, LENGTH = 263K
  RETAINED (rwx) : ORIGIN = 0x20041F00, LENGTH = 0x100
}

SECTIONS {
  .boot2 ORIGIN(BOOT2) : {
    KEEP(*(.boot2));
  } > BOOT2

  .uninit.ferrite (NOLOAD) : {
    . = ALIGN(4);
    _ferrite_retained_start = .;
    KEEP(*(.uninit.ferrite))
    _ferrite_retained_end = .;
    . = ALIGN(4);
  } > RETAINED
}
