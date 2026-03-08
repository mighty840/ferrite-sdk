# STM32F4

The STM32F4 family includes Cortex-M4/M4F microcontrollers from STMicroelectronics. This page covers the STM32F4xx series (e.g., STM32F401, STM32F407, STM32F411, STM32F429).

## Memory layout

SRAM sizes vary across the STM32F4 family:

| Part | SRAM | Address range |
|---|---|---|
| STM32F401 | 64 KB + 32 KB | `0x20000000` - `0x20017FFF` |
| STM32F407 | 128 KB + 64 KB CCM | `0x20000000` - `0x2001FFFF` (main) |
| STM32F411 | 128 KB | `0x20000000` - `0x2001FFFF` |
| STM32F429 | 256 KB | `0x20000000` - `0x2003FFFF` |

### Linker script (STM32F411, 128 KB)

```ld
MEMORY
{
  FLASH    : ORIGIN = 0x08000000, LENGTH = 512K
  RAM      : ORIGIN = 0x20000000, LENGTH = 127K   /* 128K - 256 bytes */
  RETAINED (rwx) : ORIGIN = 0x2001FF00, LENGTH = 0x100
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

The pre-built fragment at `linker/stm32f4-retained.x` uses the 128 KB SRAM1 layout.

::: warning
Do **not** place the retained block in CCM (Core Coupled Memory) on parts like the STM32F407. CCM is at `0x10000000` and is not retained across all reset types. Use the main SRAM region.
:::

### RAM regions

For STM32F407 with 128 KB main SRAM:

```rust
ram_regions: &[RamRegion {
    start: 0x2000_0000,
    end: 0x2002_0000,  // 128 KB main SRAM
}],
```

If your application also uses CCM RAM, add it as a second region:

```rust
ram_regions: &[
    RamRegion { start: 0x2000_0000, end: 0x2002_0000 },
    RamRegion { start: 0x1000_0000, end: 0x1001_0000 },  // 64 KB CCM
],
```

## Reset reason

The STM32F4 reset cause is in the RCC CSR register (`0x40023874`):

```rust
fn read_stm32f4_reset_reason() -> ferrite_sdk::RebootReason {
    let rcc = unsafe { &*pac::RCC::ptr() };
    let csr = rcc.csr.read();

    let reason = if csr.lpwrrstf().bit_is_set() {
        ferrite_sdk::RebootReason::BrownoutReset
    } else if csr.wwdgrstf().bit_is_set() {
        ferrite_sdk::RebootReason::WatchdogTimeout
    } else if csr.iwdgrstf().bit_is_set() {
        ferrite_sdk::RebootReason::WatchdogTimeout
    } else if csr.sftrstf().bit_is_set() {
        ferrite_sdk::RebootReason::SoftwareReset
    } else if csr.porrstf().bit_is_set() {
        ferrite_sdk::RebootReason::PowerOnReset
    } else if csr.pinrstf().bit_is_set() {
        ferrite_sdk::RebootReason::PinReset
    } else {
        ferrite_sdk::RebootReason::Unknown
    };

    // Clear reset flags by setting RMVF bit
    rcc.csr.modify(|_, w| w.rmvf().set_bit());

    reason
}
```

## Build

```bash
rustup target add thumbv7em-none-eabihf
cargo build --target thumbv7em-none-eabihf --features cortex-m --release
```

For parts without hardware FPU (e.g., STM32F401 in some configurations):

```bash
rustup target add thumbv7em-none-eabi
cargo build --target thumbv7em-none-eabi --features cortex-m --release
```

## Flashing

```bash
# With probe-rs (ST-Link or CMSIS-DAP)
cargo run --release

# Or using probe-rs directly
probe-rs run --chip STM32F411CEUx target/thumbv7em-none-eabihf/release/my-firmware
```

## Retained RAM behavior

SRAM on STM32F4 is retained across software resets, watchdog resets, and pin resets. It is cleared on power-on reset (POR) and brownout reset (BOR). This matches the expected behavior for ferrite-sdk.
