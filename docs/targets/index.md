# Target Platforms

ferrite-sdk supports ARM Cortex-M3, M4, and M4F processors. Each target needs a linker script fragment to reserve retained RAM at a target-specific address.

## Supported targets

| Target | Rust target triple | Pre-built linker fragment |
|---|---|---|
| [nRF52840](./nrf52840) | `thumbv7em-none-eabihf` | `linker/nrf52840-retained.x` |
| [RP2040](./rp2040) | `thumbv6m-none-eabi` | `linker/rp2040-retained.x` |
| [STM32F4](./stm32f4) | `thumbv7em-none-eabihf` | `linker/stm32f4-retained.x` |

## Porting to a new target

To use ferrite-sdk on an unsupported Cortex-M microcontroller:

1. **Identify the SRAM layout.** Find the start and end addresses of the main SRAM region in the datasheet.
2. **Reserve 256 bytes** at the end of SRAM for the retained block. Adjust your `MEMORY` block so the main RAM region ends 256 bytes earlier.
3. **Create a linker fragment** with a `RETAINED` region and the `.uninit.ferrite` section (see the existing fragments for the pattern).
4. **Read the reset-cause register** for your MCU and map it to a `RebootReason` variant.
5. **Verify retained RAM survives soft resets.** Flash a test firmware that writes a magic value to retained RAM, triggers a software reset, and checks whether the value persists.

## What varies per target

| Aspect | Target-specific |
|---|---|
| Retained RAM address | Yes -- depends on SRAM layout |
| Reset-cause register | Yes -- different peripheral address and bit layout |
| RAM regions for fault handler | Yes -- pass your SRAM range(s) to `SdkConfig.ram_regions` |
| Probe tool | Usually probe-rs, but some targets need vendor tools |
| Linker script format | Mostly identical, just different addresses |

Everything else (SDK init, metrics, transport, upload) is the same across all targets.
