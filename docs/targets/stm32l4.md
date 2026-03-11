# STM32L4

The STM32L4 family includes ultra-low-power Cortex-M4F microcontrollers from STMicroelectronics. This page covers the STM32L4A6 specifically (Nucleo-L4A6ZG board), but applies broadly to the STM32L4x6 series.

## Memory layout

| Part | SRAM | Address range |
|---|---|---|
| STM32L4A6 | 320 KB (SRAM1 256 KB + SRAM2 64 KB) | `0x20000000` - `0x2004FFFF` |
| STM32L476 | 128 KB (SRAM1 96 KB + SRAM2 32 KB) | `0x20000000` - `0x2001FFFF` |
| STM32L496 | 320 KB (SRAM1 256 KB + SRAM2 64 KB) | `0x20000000` - `0x2004FFFF` |

### Linker script (STM32L4A6, 320 KB)

```ld
MEMORY
{
  FLASH    : ORIGIN = 0x08000000, LENGTH = 1024K
  RAM      : ORIGIN = 0x20000000, LENGTH = 319K   /* 320K - 1K reserved */
  RETAINED (rwx) : ORIGIN = 0x2004FC00, LENGTH = 0x400
}

SECTIONS
{
  .uninit.ferrite (NOLOAD) : {
    . = ALIGN(4);
    _ferrite_retained_start = .;
    KEEP(*(.uninit.ferrite))
    _ferrite_retained_end = .;
    . = ALIGN(4);
  } > RETAINED
}
```

The pre-built fragment at `linker/stm32l4-retained.x` uses the 320 KB SRAM layout (STM32L4A6/L496).

::: warning
SRAM2 (64 KB at `0x10000000`) on STM32L4x6 parts is a separate memory region. It is retained across resets and is actually a good choice for the retained block. The default linker fragment places the retained block at the end of SRAM1+SRAM2 (mapped contiguously at `0x20000000`).
:::

### RAM regions

For STM32L4A6 with 320 KB SRAM:

```rust
ram_regions: &[RamRegion {
    start: 0x2000_0000,
    end: 0x2005_0000,  // 320 KB (SRAM1 + SRAM2)
}],
```

## Reset reason

The STM32L4 reset cause is in the RCC CSR register (`0x40021094`):

```rust
const RCC_CSR: *mut u32 = 0x4002_1094 as *mut u32;

fn read_stm32l4_reset_reason() -> ferrite_sdk::RebootReason {
    let csr = unsafe { core::ptr::read_volatile(RCC_CSR) };

    // Clear reset flags by setting RMVF (bit 23)
    unsafe {
        core::ptr::write_volatile(RCC_CSR, csr | (1 << 23));
    }

    if csr & (1 << 31) != 0 {
        ferrite_sdk::RebootReason::WatchdogTimeout // WWDGRSTF
    } else if csr & (1 << 30) != 0 {
        ferrite_sdk::RebootReason::WatchdogTimeout // IWDGRSTF
    } else if csr & (1 << 29) != 0 {
        ferrite_sdk::RebootReason::SoftwareReset   // SFTRSTF
    } else if csr & (1 << 28) != 0 {
        ferrite_sdk::RebootReason::PowerOnReset    // BORRSTF
    } else if csr & (1 << 27) != 0 {
        ferrite_sdk::RebootReason::PinReset        // PINRSTF
    } else {
        ferrite_sdk::RebootReason::Unknown
    }
}
```

## Build

```bash
rustup target add thumbv7em-none-eabihf
cargo build --target thumbv7em-none-eabihf --features cortex-m --release
```

## Flashing

```bash
# With probe-rs (ST-Link)
cargo run --release

# Or directly
probe-rs run --chip STM32L4A6ZGTx target/thumbv7em-none-eabihf/release/my-firmware
```

## Board examples

Three external example repos demonstrate ferrite-sdk on the Nucleo-L4A6ZG:

| Repo | Framework | Description |
|---|---|---|
| [ferrite-nucleo-l4a6zg](https://github.com/mighty840/ferrite-nucleo-l4a6zg) | Embassy | Async tasks, embassy-time ticks |
| [ferrite-nucleo-l4a6zg-baremetal](https://github.com/mighty840/ferrite-nucleo-l4a6zg-baremetal) | cortex-m-rt | SysTick + superloop, raw GPIO |
| [ferrite-nucleo-l4a6zg-rtic](https://github.com/mighty840/ferrite-nucleo-l4a6zg-rtic) | RTIC v1 | Hardware tasks, systick-monotonic |

All three use RTT (via probe-rs) for chunk transport and include a Python bridge script (`rtt_bridge.py`) that forwards chunks to the ferrite-server.

## Known issues

### VCP serial port

The Nucleo-L4A6ZG's ST-LINK VCP is wired to LPUART1 (PG7 TX / PG8 RX). Some board revisions have unpopulated solder bridges (SB13/SB14) that prevent VCP data from reaching the host. The RTT-based bridge avoids this by using the debug probe (SWD) for data transport.

### Embassy version pinning

The ferrite-sdk depends on `embassy-time 0.3`. Embassy example firmware must use compatible versions (embassy-stm32 0.1, embassy-executor 0.5). Newer embassy versions cause `embassy-time-driver` linker conflicts.

## Retained RAM behavior

SRAM on STM32L4 is retained across software resets, watchdog resets, and pin resets. It is cleared on power-on reset (POR) and brownout reset (BOR). This matches the expected behavior for ferrite-sdk.
