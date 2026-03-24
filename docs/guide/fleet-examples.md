# Fleet Examples — Board Bring-Up Guide

This guide documents the 5-board fleet demo and all the hardware/software pitfalls discovered during bring-up. Each board uses a different firmware stack and transport, demonstrating ferrite-sdk's portability.

## Fleet Overview

| Board | MCU | Target | Stack | Transport | Gateway Path |
|-------|-----|--------|-------|-----------|-------------|
| [ESP32-C3](#esp32-c3-wifi-http) | RISC-V | `riscv32imc-unknown-none-elf` | Embassy (esp-hal 0.23) | WiFi → HTTP | Direct to server |
| [STM32L4A6](#stm32l4a6-usb-cdc) | Cortex-M4 | `thumbv7em-none-eabihf` | Embassy (embassy-stm32 0.1) | USB CDC | → Gateway serial → Server |
| [STM32WL55](#stm32wl55-usart-vcp) | Cortex-M4 (no FPU) | `thumbv7em-none-eabi` | RTIC 2.x (stm32wl PAC) | LPUART VCP | → Gateway serial → Server |
| [STM32H563](#stm32h563-ethernet) | Cortex-M33 | `thumbv8m.main-none-eabihf` | Embassy (embassy-stm32 0.1) | Ethernet HTTP | → Gateway HTTP:4001 → Server |
| [nRF5340](#nrf5340-ble) | Cortex-M33 (no FPU) | `thumbv8m.main-none-eabi` | Zephyr 4.1 (C + FFI) | BLE GATT | → Gateway BLE → Server |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Host Machine                                                           │
│  ┌──────────────────┐  ┌────────────────────────────────────┐           │
│  │ ferrite-server    │  │ ferrite-dashboard (dx serve)       │           │
│  │ :4000             │  │ :8080 → proxy to :4000             │           │
│  │ SQLite + REST API │  │ Dioxus WASM                        │           │
│  └────────▲──────────┘  └────────────────────────────────────┘           │
│           │                                                              │
└───────────┼──────────────────────────────────────────────────────────────┘
            │ HTTP
┌───────────┼──────────────────────────────────────────────────────────────┐
│  Raspberry Pi (Edge Gateway)                                             │
│  ┌────────┴───────────────────────────────────────────────────────┐      │
│  │ ferrite-gateway                                                │      │
│  │   --usb-port /dev/ttyACM0    (WL55 VCP)                       │      │
│  │   --usb-port /dev/ttyACM1    (L4A6 USB CDC)                   │      │
│  │   --http-ingest-port 4001    (H563 Ethernet)                  │      │
│  │   --ble                      (nRF5340 BLE GATT)               │      │
│  │   --server http://host:4000                                    │      │
│  └────▲──────────▲──────────────▲───────────────────▲─────────────┘      │
│       │          │              │                   │                    │
└───────┼──────────┼──────────────┼───────────────────┼────────────────────┘
        │USB       │USB           │Ethernet           │BLE
   ┌────┴────┐ ┌───┴──────┐  ┌───┴────────┐  ┌──────┴───────┐
   │WL55JC1  │ │L4A6ZG    │  │H563ZI      │  │nRF5340-DK    │
   │RTIC     │ │Embassy   │  │Embassy     │  │Zephyr C+FFI  │
   │LPUART   │ │USB CDC   │  │Ethernet    │  │BLE GATT      │
   └─────────┘ └──────────┘  └────────────┘  └──────────────┘

   ┌──────────────────┐
   │ESP32-C3          │───WiFi HTTP POST──────────────────────▶ server:4000
   │Embassy (esp-hal) │   (bypasses gateway)
   └──────────────────┘
```

---

## ESP32-C3 (WiFi/HTTP)

**Example:** [`examples/embassy-esp32c3/`](https://github.com/mighty840/ferrite-sdk/tree/main/examples/embassy-esp32c3)

### Key Details
- **Device ID:** `esp32c3-embassy-01`
- **Crate ecosystem:** esp-hal 0.23 + esp-hal-embassy 0.6 + esp-wifi 0.12 + embassy-net 0.6 (all use smoltcp 0.12 + embassy-time 0.4)
- **Toolchain:** `nightly-2025-04-15` (pinned in `rust-toolchain.toml`)
- **Transport:** `BufferingTransport` — collects chunks synchronously via `UploadManager::upload()`, then POSTs each chunk over async TCP
- **Config:** Build-time `.env` file (loaded by `build.rs`) — `WIFI_SSID`, `WIFI_PASS`, `SERVER_HOST`

### Pitfalls

#### ESP ecosystem version deadlock
The esp-rs crates on crates.io have severe version incompatibilities:
- `esp-hal 1.0.0` (final) removed `__esp_hal_embassy` and `__esp_wifi_builtin_scheduler` features that `esp-hal-embassy 0.9.x` and `esp-wifi 0.15.x` depend on
- `smoltcp 0.12` has non-exhaustive enum match errors on Rust 1.94+

**Solution:** Pin to `esp-hal 0.23` + `esp-hal-embassy 0.6` + `esp-wifi 0.12` + `embassy-net 0.6` (all use smoltcp 0.12 + embassy-time 0.4). Pin Rust to `nightly-2025-04-15`.

#### embassy-time 0.3 vs 0.4 conflict
ferrite-sdk workspace pins `embassy-time = "0.3"`, but esp-hal-embassy 0.6 needs `embassy-time = "0.4"`. Since the ESP example is excluded from the workspace, the ferrite-sdk path dep resolves its own `embassy-time 0.3` separately. This causes the `embassy-time-driver` singleton to conflict.

**Solution:** Don't use the `embassy` feature of ferrite-sdk. Use `BufferingTransport` (sync `ChunkTransport`) to collect chunks, then send via async TCP. This avoids pulling in ferrite-sdk's embassy-time dependency entirely.

#### ROM symbols / linker scripts
`esp-wifi` links against C ROM libraries that need ROM symbol definitions from `esp-rom-sys`. These `.ld` files must be on the linker search path.

**Solution:** Add `-Tlinkall.x` and `-Trom-functions.x` to `.cargo/config.toml` rustflags:
```toml
rustflags = [
    "-C", "link-arg=-Tlinkall.x",
    "-C", "link-arg=-Trom-functions.x",
]
```

#### Duplicate global allocator
`esp-alloc` defines `#[global_allocator]` at the crate level. If you depend on a different version than `esp-wifi`'s transitive dep, you get two allocators.

**Solution:** Match the `esp-alloc` version to what `esp-wifi` pulls in (check with `cargo tree`).

#### Flashing
The ESP32-C3 needs a merged flash image (bootloader + partition table + app):
```bash
espflash save-image --chip esp32c3 --merge target/.../release/binary /tmp/merged.bin
esptool.py --port /dev/ttyUSB0 --baud 460800 --chip esp32c3 write_flash -z 0x0 /tmp/merged.bin
```

---

## STM32L4A6 (USB CDC)

**Example:** [`examples/embassy-stm32l4a6/`](https://github.com/mighty840/ferrite-sdk/tree/main/examples/embassy-stm32l4a6)

### Key Details
- **Device ID:** `stm32l4a6-fleet-01`
- **Crate ecosystem:** embassy-stm32 0.1 + embassy-executor 0.7 + embassy-time 0.3
- **Target:** `thumbv7em-none-eabihf`
- **Clock:** HSI 16MHz sysclk (PLL configs crash — see below)
- **Transport:** USB CDC via `embassy-usb` `CdcAcmClass` → `UsbCdcTransport`

### Pitfalls

#### Embassy executor WFE wake issue (CRITICAL)
The default embassy-executor thread-mode uses `wfe` (Wait For Event) to idle. On this chip, **WFE does not wake on pending interrupts** without a debugger attached. The TIM2 interrupt fires, becomes pending in NVIC, but the CPU stays in WFE because the event register was already consumed.

**Symptoms:** Firmware works perfectly under probe-rs but freezes standalone. Timer interrupts are pending (confirmed via NVIC_ISPR), PRIMASK=0, BASEPRI=0, interrupt priority correct — yet ISR never executes.

**Solution:** Use `embassy_executor::raw::Executor` with a manual spin-poll loop:
```rust
static EXECUTOR: StaticCell<embassy_executor::raw::Executor> = StaticCell::new();

#[entry]
fn main() -> ! {
    let p = embassy_stm32::init(config);
    let executor = EXECUTOR.init(embassy_executor::raw::Executor::new(
        cortex_m::asm::sev as *mut ()
    ));
    unsafe { executor.spawner().spawn(main_task(p)).unwrap(); }
    loop { unsafe { executor.poll() }; }  // spin-poll, no WFI/WFE
}
```

#### panic-probe causes reset loop without debugger
`panic-probe` executes a `bkpt` instruction on panic. Without a debugger, this triggers HardFault → reset → boot → panic → infinite loop.

**Solution:** Define a custom panic handler:
```rust
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop { cortex_m::asm::nop(); }
}
```

#### Task arena size
Embassy-executor 0.7's `raw::Executor` has a fixed task arena. USB CDC tasks are large (~32KB). Default arena (4KB) overflows silently.

**Solution:** Add `task-arena-size-65536` feature:
```toml
embassy-executor = { version = "0.7", features = ["arch-cortex-m", "executor-thread", "task-arena-size-65536"] }
```

#### Embassy-time generic-queue
Embassy-executor 0.7 removed `integrated-timers`. You must add `generic-queue-8` to `embassy-time`:
```toml
embassy-time = { version = "0.3", features = ["generic-queue-8"] }
```

#### MSI 48MHz clock crashes
Setting `MSIRange::RANGE48M` via embassy-stm32 RCC config causes a crash during init. HSI 16MHz with no PLL works reliably. MSI 48MHz for USB clock works when used alongside HSI sysclk (not as PLL source).

#### LED pins vary by board revision
PB0 is documented as LD1 (green) on Nucleo-144 boards, but **some L4A6ZG revisions don't connect PB0 to LD1**. PB7 (blue/LD2) and PB14 (red/LD3) work reliably.

**Debugging tip:** Use busy-wait NOP blink to test GPIOs before involving embassy timers.

#### USB CDC DTR requirement
The `UsbCdcTransport` checks `dtr()` before sending. The gateway must set DTR when opening the serial port, otherwise the device reports `TransportUnavailable`.

**Gateway fix:** `port.write_data_terminal_ready(true)` after opening.

---

## STM32WL55 (USART VCP)

**Example:** [`examples/rtic-stm32wl55/`](https://github.com/mighty840/ferrite-sdk/tree/main/examples/rtic-stm32wl55)

### Key Details
- **Device ID:** `stm32wl55-rtic-01`
- **Crate ecosystem:** RTIC 2.x + stm32wl 0.15 PAC + rtic-monotonics 2.x
- **Target:** `thumbv7em-none-eabi` (soft-float — **no FPU on WL55 CM4**)
- **Clock:** HSE 32MHz TCXO
- **Transport:** LPUART1 (PA2/PA3 AF8) → ST-LINK VCP → `/dev/ttyACMx`

### Pitfalls

#### No FPU on STM32WL55 CM4 core (CRITICAL)
The STM32WL55's Cortex-M4 core does **NOT** have an FPU. Using `thumbv7em-none-eabihf` (hard-float) compiles fine but crashes at runtime with "Coprocessor access error" on any float operation in an interrupt context.

**Solution:** Use `thumbv7em-none-eabi` (soft-float). Replace all floating-point operations with integer math:
```rust
// BAD: let temp = 25.0 + (count % 20) as f32 * 0.3;
// GOOD:
let temp = 25 + (count % 20) * 3 / 10;
```

#### LPUART1, not USART2, for VCP
On NUCLEO-WL55JC1, the ST-LINK VCP is wired to **LPUART1** (PA2/PA3 at AF8), NOT USART2 (which would be AF7 on the same pins). Using USART2 sends data nowhere.

**Solution:** Enable LPUART1 clock on `RCC.apb1enr2`, use AF8, and use LPUART BRR formula:
```rust
// LPUART BRR = 256 * fclk / baud = 256 * 32000000 / 115200 = 71111
dp.LPUART.brr.write(|w| unsafe { w.bits(71111) });
```

Also use `txfnf()` instead of `txe()` for the TX-ready flag.

#### stm32wl 0.15 PAC field-style access
The `stm32wl` 0.15 PAC uses **field access** (`.isr`, `.tdr`, `.brr`), not method calls (`.isr()`, `.tdr()`, `.brr()`).

#### `.cargo/config.toml` rustflags not applied
When an example is excluded from the workspace, Cargo may not read `.cargo/config.toml` from the example directory. Linker scripts (`-Tlink.x`) specified there won't be applied.

**Solution:** Emit link args from `build.rs` instead:
```rust
fn main() {
    println!("cargo:rustc-link-arg=-Tlink.x");
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    println!("cargo:rustc-link-search=.");
}
```

#### RTIC dispatcher interrupts
RTIC 2.x software tasks need interrupt dispatchers. On WL55, `SPI2` interrupt is named `SPI2S2`:
```rust
#[rtic::app(... dispatchers = [SPI1, SPI2S2])]
```

#### RDP protection on new boards
Fresh or previously-locked WL55 boards may have RDP level 1 enabled. `probe-rs` and `openocd` can't connect without erasing first.

**Solution:**
```bash
st-flash --connect-under-reset erase
```
Or with OpenOCD:
```bash
openocd -f interface/stlink-dap.cfg -c 'reset_config srst_only connect_assert_srst' \
  -f target/stm32wlx.cfg -c 'init; stm32wlx unlock 0; exit'
```

---

## STM32H563 (Ethernet)

**Example:** [`examples/embassy-stm32h563/`](https://github.com/mighty840/ferrite-sdk/tree/main/examples/embassy-stm32h563)

### Key Details
- **Device ID:** `stm32h563-fleet-01`
- **Crate ecosystem:** embassy-stm32 0.1 + embassy-net 0.4
- **Target:** `thumbv8m.main-none-eabihf`
- **Clock:** HSE 8MHz bypass → PLL1 248MHz
- **PHY:** LAN8742A (RMII, on-board)
- **Transport:** `BufferingTransport` → raw HTTP POST to gateway:4001
- **Static IP:** 192.168.0.200 (no DHCP server on RPi)

### Pitfalls

#### RMII pin mapping
The Nucleo-H563ZI RMII pins are:
```
PA1  = REF_CLK    PA2  = MDIO      PC1  = MDC
PA7  = CRS_DV     PC4  = RXD0      PC5  = RXD1
PG13 = TXD0       PB15 = TXD1      PG11 = TX_EN
```
**PB13 is NOT TXD1** (common mistake from other STM32 boards). Check the Nucleo schematic.

#### embassy-net 0.4 API differences
- `Stack::new()` instead of `embassy_net::new()`
- `Stack::run()` instead of separate `Runner`
- `TcpSocket::new()` takes `&Stack<D>` by reference

#### RCC_RSR address
The H563 RCC base is `0x44020C00`, RSR offset is `0x0D0` → address `0x44020CD0`. Other STM32 families use different addresses. A wrong address causes BusFault immediately.

#### Direct Ethernet to RPi (no DHCP)
When connecting the H563 directly to the RPi's Ethernet port, there's no DHCP server. Use a static IP:
```rust
let net_config = Config::ipv4_static(embassy_net::StaticConfigV4 {
    address: embassy_net::Ipv4Cidr::new(Ipv4Address::new(192, 168, 0, 200), 24),
    gateway: Some(Ipv4Address::new(192, 168, 0, 1)),
    dns_servers: heapless::Vec::new(),
});
```

#### Gateway HTTP ingest endpoint
The gateway's default mode only reads serial ports and BLE. For Ethernet devices, the gateway has an HTTP ingest endpoint:
```bash
ferrite-gateway --http-ingest-port 4001
```
Devices POST to `http://gateway-ip:4001/ingest/chunks`.

---

## nRF5340 (BLE)

**Example:** [`examples/zephyr-nrf5340/`](https://github.com/mighty840/ferrite-sdk/tree/main/examples/zephyr-nrf5340)

### Key Details
- **Device ID:** `nrf5340-zephyr-01`
- **Stack:** Zephyr 4.1 RTOS (C) + ferrite-sdk C FFI (`libferrite_ffi.a`)
- **Target:** `thumbv8m.main-none-eabi` (soft-float — no FPU on app core)
- **Transport:** BLE GATT notifications → gateway `btleplug` scanner
- **BLE Service UUID:** `FE771E00-0001-4000-8000-00805F9B34FB`
- **BLE Chunk Characteristic:** `FE771E00-0002-4000-8000-00805F9B34FB`

### Pitfalls

#### Dual-core: network core needs separate firmware (CRITICAL)
The nRF5340 has two cores. The application core runs your firmware, but **BLE requires the network core to be programmed with a BLE controller**. Without it, `bt_enable()` blocks forever.

**Solution:** Build and flash the HCI IPC sample for the network core:
```bash
west build -b nrf5340dk/nrf5340/cpunet -d build_net zephyr/samples/bluetooth/hci_ipc
nrfjprog --program build_net/zephyr/zephyr.hex -f NRF53 --coprocessor CP_NETWORK --sectorerase --reset
```

#### APPROTECT / device security
Fresh nRF5340 boards or previously-locked devices have APPROTECT enabled. J-Link, probe-rs, and pyocd all fail to connect ("debug port ID 0, expected 6").

**Solution:** Use `nrfjprog --recover`:
```bash
nrfjprog --recover -f NRF53
```
This mass-erases both cores and clears APPROTECT. Requires SEGGER J-Link software + Nordic nRF Command Line Tools.

#### J-Link, not ST-LINK
The nRF5340-DK uses a **J-Link OB** debug probe (SEGGER), not ST-LINK. `probe-rs` 0.31 does NOT support the nRF5340's multi-drop SWD. `pyocd` also fails. You need:
- **SEGGER J-Link Software** (`JLinkExe`) — for the J-Link driver
- **Nordic nRF Command Line Tools** (`nrfjprog`) — for nRF-specific operations (recover, erase, program)

#### No FPU on nRF5340 app core
Like the STM32WL55, the nRF5340's app core Cortex-M33 does NOT have an FPU. The FFI library must be built for `thumbv8m.main-none-eabi` (soft-float):
```bash
cargo build -p ferrite-ffi --release --target thumbv8m.main-none-eabi
```

#### Critical-section stubs for C host
The ferrite-sdk Rust FFI library uses the `critical-section` crate. In a Zephyr host, the C code must provide these symbols:
```c
bool _critical_section_1_0_acquire(void) {
    _cs_key = irq_lock();
    return true;
}
void _critical_section_1_0_release(bool _token) {
    irq_unlock(_cs_key);
}
```

#### cbindgen naming convention
cbindgen with `prefix_with_name = true` generates `FERRITE_ERROR_T_OK` (not `FERRITE_ERROR_OK`). The `_T_` comes from the typedef name `ferrite_error_t`.

#### BLE advertisement: name must be in ad packet
Older BlueZ versions (RPi Buster/Bullseye) don't populate service UUIDs from BLE advertisement data during passive scan. The gateway matches devices by **local name** as a fallback.

**Solution:** Put the device name in the advertisement packet (not just scan response):
```c
static const struct bt_data ad[] = {
    BT_DATA_BYTES(BT_DATA_FLAGS, (BT_LE_AD_GENERAL | BT_LE_AD_NO_BREDR)),
    BT_DATA(BT_DATA_NAME_COMPLETE, DEVICE_ID, sizeof(DEVICE_ID) - 1),
};
```

#### Zephyr build environment
Zephyr requires a dedicated workspace:
```bash
pip3 install west
mkdir ~/zephyrproject && cd ~/zephyrproject
west init -m https://github.com/zephyrproject-rtos/zephyr --mr v4.1.0
west update --narrow --fetch-opt=--depth=1
pip3 install -r zephyr/scripts/requirements.txt

# Install Zephyr SDK (ARM toolchain)
wget https://github.com/zephyrproject-rtos/sdk-ng/releases/download/v0.17.0/zephyr-sdk-0.17.0_linux-x86_64_minimal.tar.xz
tar xf zephyr-sdk-0.17.0_linux-x86_64_minimal.tar.xz
cd zephyr-sdk-0.17.0 && ./setup.sh -t arm-zephyr-eabi
```

Build from the Zephyr workspace directory:
```bash
cd ~/zephyrproject
west build -b nrf5340dk/nrf5340/cpuapp /path/to/examples/zephyr-nrf5340
```

---

## C FFI Example (STM32L4A6)

**Example:** [`examples/c-stm32l4a6/`](https://github.com/mighty840/ferrite-sdk/tree/main/examples/c-stm32l4a6)

This is a bare-metal C firmware using ferrite-sdk via FFI bindings. Not part of the live fleet demo (the L4A6 runs Embassy USB CDC instead) but demonstrates C integration.

### Building the FFI library
```bash
# Generate C header
cbindgen --config ferrite-ffi/cbindgen.toml --crate ferrite-ffi --output include/ferrite-sdk.h

# Build static library
cargo build -p ferrite-ffi --release --target thumbv7em-none-eabihf

# Build C firmware
cd examples/c-stm32l4a6 && make
```

### Pitfalls
- `cbindgen` requires `[parse.expand]` removed (needs nightly). Without it, all FFI types must be defined directly in `ferrite-ffi/src/lib.rs` (not re-exported from other crates).
- Critical-section stubs must be provided by the C code (same as Zephyr example).
- The FFI library path is `../../target/thumbv7em-none-eabihf/release/libferrite_ffi.a` (workspace target dir, not crate-local).

---

## Gateway Configuration

### Cross-compilation for RPi (armv7)
```bash
# Install cross
cargo install cross

# Cross.toml provides libudev-dev and libdbus-1-dev for serial + BLE
cross build -p ferrite-gateway --target armv7-unknown-linux-gnueabihf --release
```

The `Cross.toml` in the repo root configures the Docker container:
```toml
[target.armv7-unknown-linux-gnueabihf]
pre-build = [
    "dpkg --add-architecture armhf",
    "apt-get update",
    "apt-get install -y libudev-dev:armhf libdbus-1-dev:armhf pkg-config"
]
```

### reqwest TLS for cross-compile
The gateway uses `reqwest` with `rustls-tls` (not `native-tls`) to avoid OpenSSL cross-compilation issues:
```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
```

### Chunk batching
The gateway batches chunks from the same upload session (200ms window) into a single HTTP POST. Without batching, metrics chunks arrive without a DeviceInfo chunk and get assigned to the "unknown" device.

### Device status
The server sets status to "online" on heartbeat. For auto-discovered devices (no `device_key`), the fix in `ingest.rs` calls `update_device_status_by_id()` on every heartbeat to keep the dashboard green.

---

## Flashing Quick Reference

| Board | Flasher | Command |
|-------|---------|---------|
| ESP32-C3 | esptool.py | `esptool.py --port /dev/ttyUSB0 --baud 460800 --chip esp32c3 write_flash -z 0x0 merged.bin` |
| STM32L4A6 | probe-rs | `probe-rs download --chip STM32L4A6ZGTx firmware.elf && probe-rs reset --chip STM32L4A6ZGTx` |
| STM32WL55 | st-flash | `st-flash --connect-under-reset write firmware.bin 0x8000000` |
| STM32H563 | probe-rs | `probe-rs download --chip STM32H563ZITx firmware.elf && probe-rs reset --chip STM32H563ZITx` |
| nRF5340 | nrfjprog | `nrfjprog --program zephyr.hex -f NRF53 --coprocessor CP_APPLICATION --sectorerase --reset` |

### Remote flashing via RPi (OpenOCD)
For boards connected to the RPi (not the host), use OpenOCD over SSH:
```bash
sshpass -p <pass> ssh pi@rpi "sudo openocd -f interface/stlink.cfg \
  -f target/stm32l4x.cfg \
  -c 'program /path/to/firmware.elf verify reset exit'"
```

**Note:** OpenOCD on RPi (Bullseye) only works with ST-LINK V2/V3. For the WL55 (ST-LINK V3), use `stlink-dap.cfg` instead of `stlink.cfg` and add `reset_config srst_only connect_assert_srst`.

---

## Common Pitfalls Summary

| Pitfall | Affects | Fix |
|---------|---------|-----|
| Embassy WFE doesn't wake without debugger | L4A6, potentially all Cortex-M | Use `raw::Executor` spin-poll loop |
| `panic-probe` reset loop without debugger | All embedded boards | Custom `#[panic_handler]` with `loop { nop() }` |
| No FPU on chip | WL55 (CM4), nRF5340 (CM33) | Use `-eabi` target (soft-float), no float ops |
| Wrong UART for VCP | WL55 (LPUART1 not USART2) | Check board schematic for VCP routing |
| RDP / APPROTECT locked | WL55 (RDP1), nRF5340 (APPROTECT) | `st-flash --connect-under-reset erase` / `nrfjprog --recover` |
| Embassy-time version conflict | ESP32-C3 | Drop `embassy` feature, use `BufferingTransport` |
| Duplicate `memory.x` | H563, L4A6 | Put link args in `build.rs`, not both `build.rs` + `.cargo/config.toml` |
| USB CDC no data (DTR) | L4A6 via gateway | Gateway must `write_data_terminal_ready(true)` |
| BLE device not discovered | nRF5340 via gateway | Put device name in ad packet, not just scan response |
| Gateway "unknown" device | All via gateway | Batch chunks (200ms window) to preserve DeviceInfo context |
| cbindgen `_T_` naming | C FFI examples | Use `FERRITE_ERROR_T_OK` not `FERRITE_ERROR_OK` |
