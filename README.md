<p align="center">
  <img src="docs/public/logo.svg" alt="iotai-sdk logo" width="120" />
</p>

<h1 align="center">iotai-sdk</h1>

<p align="center">
  <strong>Firmware observability for ARM Cortex-M — crashes, metrics, and logs with zero alloc.</strong>
</p>

<p align="center">
  <a href="https://github.com/your-org/iotai-sdk/actions"><img src="https://img.shields.io/github/actions/workflow/status/your-org/iotai-sdk/ci.yml?branch=main&style=flat-square&logo=github&label=CI" alt="CI"></a>
  <a href="https://crates.io/crates/iotai-sdk"><img src="https://img.shields.io/crates/v/iotai-sdk?style=flat-square&logo=rust&label=crates.io" alt="crates.io"></a>
  <a href="https://docs.rs/iotai-sdk"><img src="https://img.shields.io/docsrs/iotai-sdk?style=flat-square&logo=docs.rs&label=docs.rs" alt="docs.rs"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue?style=flat-square" alt="License"></a>
  <a href="https://your-org.github.io/iotai-sdk/"><img src="https://img.shields.io/badge/docs-VitePress-646CFF?style=flat-square&logo=vitepress" alt="Docs"></a>
</p>

---

## What It Does

iotai-sdk captures everything you need to debug embedded devices in the field:

- **HardFault capture** — all Cortex-M registers, CFSR/HFSR, 64-byte stack snapshot
- **Reboot reason tracking** — power-on, watchdog, fault, brownout, software reset
- **Metrics** — counters, gauges, histograms in a fixed-capacity ring buffer
- **Trace logs** — defmt output captured and uploaded as binary fragments
- **Transport agnostic** — UART, BLE, LoRa, USB CDC, HTTP — implement one trait

Data survives reboots via retained RAM. The companion server stores, decodes, and symbolicates everything.

## Memory Footprint

Default configuration: **~1.7 KB RAM, ~6 KB flash**. No alloc, no std, no panics.

## Quickstart (Embassy + nRF52840)

**1. Add dependencies:**

```toml
[dependencies]
iotai-sdk = { version = "0.1", features = ["cortex-m", "defmt", "embassy"] }
iotai-sdk-embassy = "0.1"
```

**2. Add linker fragment** (see [`linker/nrf52840-retained.x`](linker/nrf52840-retained.x))

**3. Initialize the SDK:**

```rust
iotai_sdk::init(SdkConfig {
    device_id: "sensor-42",
    firmware_version: env!("CARGO_PKG_VERSION"),
    build_id: 0,
    ticks_fn: || embassy_time::Instant::now().as_ticks(),
    ram_regions: &[RamRegion { start: 0x20000000, end: 0x20040000 }],
});
```

**4. Record telemetry:**

```rust
iotai_sdk::metric_gauge!("temperature", 23.5);
iotai_sdk::metric_increment!("packets_sent");
defmt::info!("system started");
```

**5. Upload periodically:**

```rust
#[embassy_executor::task]
async fn upload(transport: MyUart) -> ! {
    iotai_sdk_embassy::upload_task::upload_loop(transport, Duration::from_secs(60)).await
}
```

## Supported Targets

| Target | Architecture | Example |
|--------|-------------|---------|
| nRF52840 | Cortex-M4F | `examples/embassy-nrf52840` |
| STM32F4 | Cortex-M4F | Linker script included |
| RP2040 | Cortex-M0+ | Linker script included |

All `thumbv7m-none-eabi`, `thumbv7em-none-eabi`, and `thumbv7em-none-eabihf` targets are supported.

## Repository Structure

| Crate | Description |
|-------|-------------|
| [`iotai-sdk`](iotai-sdk/) | Core `no_std` SDK — crashes, metrics, trace, chunks |
| [`iotai-sdk-embassy`](iotai-sdk-embassy/) | Embassy async upload task |
| [`iotai-sdk-rtic`](iotai-sdk-rtic/) | RTIC resource wrapper + blocking upload |
| [`iotai-sdk-ffi`](iotai-sdk-ffi/) | C FFI static library for Zephyr/FreeRTOS |
| [`iotai-server`](iotai-server/) | Companion HTTP server + CLI + SQLite |
| [`iotai-dashboard`](iotai-dashboard/) | Dioxus WASM web dashboard |

## Build

```bash
# Host tests (no embedded toolchain needed)
cargo build -p iotai-sdk --no-default-features
cargo test -p iotai-sdk --no-default-features
cargo test -p iotai-server

# Cross-compile for Cortex-M
rustup target add thumbv7em-none-eabihf
cargo build -p iotai-sdk --features cortex-m,defmt,embassy --target thumbv7em-none-eabihf

# Run the server
cargo run -p iotai-server -- serve --port 8080

# Docs site
cd docs && npm install && npm run dev
```

## Documentation

Full documentation is available at the [VitePress docs site](https://your-org.github.io/iotai-sdk/), covering:

- [Architecture & design](docs/guide/architecture.md)
- [Binary chunk wire format](docs/reference/chunk-format.md)
- [C FFI API reference](docs/reference/c-api.md)
- [Integration guides](docs/integrations/) (Embassy, RTIC, bare-metal, Zephyr, FreeRTOS)
- [Target platform setup](docs/targets/) (nRF52840, RP2040, STM32F4)

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.
