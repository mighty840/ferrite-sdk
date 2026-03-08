# ferrite-sdk Linker Fragments

These linker script fragments define the retained RAM section used by ferrite-sdk to persist fault records and reboot reasons across resets.

## Integration with `memory.x`

1. Choose the fragment for your target MCU
2. Add the `RETAINED` memory region to your `memory.x` MEMORY block
3. Add the `.uninit.ferrite` SECTIONS block

### Example for nRF52840

```ld
MEMORY {
  FLASH : ORIGIN = 0x00000000, LENGTH = 1024K
  RAM   : ORIGIN = 0x20000000, LENGTH = 255K  /* reduced by 256 bytes */
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
```

## Important Notes

- The retained section must NOT overlap with your stack
- Reduce your main RAM length by 256 bytes to make room
- The section is marked `NOLOAD` — it is not zeroed on reset
- The SDK validates data integrity using magic numbers and CRC
