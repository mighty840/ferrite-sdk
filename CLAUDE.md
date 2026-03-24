# ferrite-sdk — Build Guide for AI Coding Agents

## Build (host, no embedded toolchain needed)
```bash
cargo build -p ferrite-sdk --no-default-features
cargo test -p ferrite-sdk --no-default-features
cargo test -p ferrite-server
```

## Build for embedded (requires ARM toolchain)
```bash
rustup target add thumbv7em-none-eabihf
cargo build -p ferrite-sdk --features cortex-m,defmt,embassy --target thumbv7em-none-eabihf
```

## Flash examples (requires probe-rs / espflash / nrfjprog)
```bash
# Embassy STM32 boards (probe-rs)
cd examples/embassy-stm32l4a6   # Nucleo-L4A6ZG (USB CDC) — thumbv7em-none-eabihf
cd examples/embassy-stm32h563   # Nucleo-H563ZI (Ethernet) — thumbv8m.main-none-eabihf
probe-rs download --chip STM32L4A6ZGTx target/.../release/binary
probe-rs reset --chip STM32L4A6ZGTx

# RTIC STM32WL55 (st-flash — probe-rs can't connect without reset)
cd examples/rtic-stm32wl55      # NUCLEO-WL55JC1 (USART VCP) — thumbv7em-none-eabi (NO FPU)
arm-none-eabi-objcopy -O binary target/.../release/binary /tmp/fw.bin
st-flash --connect-under-reset write /tmp/fw.bin 0x8000000

# ESP32-C3 (espflash — needs nightly-2025-04-15)
cd examples/embassy-esp32c3     # ESP32-C3 (WiFi/HTTP) — riscv32imc-unknown-none-elf
espflash save-image --chip esp32c3 --merge target/.../release/binary /tmp/merged.bin
esptool.py --port /dev/ttyUSB0 --chip esp32c3 write_flash -z 0x0 /tmp/merged.bin

# nRF5340-DK Zephyr (nrfjprog — requires J-Link + nRF CLI tools)
cd ~/zephyrproject
west build -b nrf5340dk/nrf5340/cpuapp /path/to/examples/zephyr-nrf5340
west build -b nrf5340dk/nrf5340/cpunet -d build_net zephyr/samples/bluetooth/hci_ipc
nrfjprog --program build/zephyr/zephyr.hex -f NRF53 --coprocessor CP_APPLICATION --sectorerase --reset
nrfjprog --program build_net/zephyr/zephyr.hex -f NRF53 --coprocessor CP_NETWORK --sectorerase --reset

# C FFI example (arm-none-eabi-gcc + cbindgen)
cd examples/c-stm32l4a6
cargo build -p ferrite-ffi --release --target thumbv7em-none-eabihf
cbindgen --config ../../ferrite-ffi/cbindgen.toml --crate ferrite-ffi --output include/ferrite-sdk.h
make
```

## Fleet demo table (5 boards, 5 stacks)
| Board | MCU | Target | Stack | Transport | Path |
|-------|-----|--------|-------|-----------|------|
| ESP32-C3 | RISC-V | riscv32imc-unknown-none-elf | Embassy (esp-hal 0.23) | WiFi → HTTP | Direct to server |
| STM32L4A6 | Cortex-M4 | thumbv7em-none-eabihf | Embassy (embassy-stm32 0.1) | USB CDC | → Gateway serial → Server |
| STM32WL55 | Cortex-M4 | thumbv7em-none-eabi | RTIC 2.x (stm32wl PAC) | LPUART VCP | → Gateway serial → Server |
| STM32H563 | Cortex-M33 | thumbv8m.main-none-eabihf | Embassy (embassy-stm32 0.1) | Ethernet | → Gateway HTTP:4001 → Server |
| nRF5340 | Cortex-M33 | thumbv8m.main-none-eabi | Zephyr 4.1 (C + FFI) | BLE GATT | → Gateway BLE → Server |

See `docs/guide/fleet-examples.md` for comprehensive pitfalls and bring-up guide.

## Run QEMU tests
```bash
rustup target add thumbv7m-none-eabi
cd tests/qemu
cargo run --release
```

## Run dashboard (development)
```bash
# Terminal 1: start the server
cargo run -p ferrite-server

# Terminal 2: start the dashboard (proxies API to server)
cd ferrite-dashboard
dx serve
```
The `dx serve` dev server proxies `/auth`, `/devices`, `/ingest`, `/health` to `localhost:4000` (see `Dioxus.toml`).

## Key design decisions
- No alloc anywhere in ferrite-sdk core
- No panics in production code paths (tests excepted)
- Feature flags gate all hardware dependencies
- cortex-m feature must be disabled for host tests
- `critical-section` crate for portable critical sections (enables host testing)
- Global state via `CriticalSectionMutex<RefCell<Option<SdkState>>>`
- Hand-rolled binary chunk encoding (postcard optional via feature)
- CRC-16/CCITT-FALSE for chunk integrity

## Authentication
- **Dual-mode**: Keycloak OIDC or Basic auth, selected automatically at server startup
- Set `KEYCLOAK_URL`, `KEYCLOAK_REALM`, `KEYCLOAK_CLIENT_ID` env vars for Keycloak mode
- Without those vars, falls back to Basic auth (default: admin/admin, configurable via `BASIC_AUTH_USER`/`BASIC_AUTH_PASS`)
- Optional `INGEST_API_KEY` env var gates `/ingest/*` endpoints
- Config loaded via `dotenvy` from `.env` file (see `.env.example`)
- Server config uses `Box::leak(Box::new(config))` for `&'static` lifetime
- Dashboard discovers auth mode at startup via `GET /auth/mode`
- Auth middleware passes OPTIONS requests through for CORS preflight support

## Transport map — ferrite-sdk
```
transport/uart.rs     → ChunkTransport (blocking, generic UART)
transport/usb_cdc.rs  → AsyncChunkTransport (embassy-usb, feature: usb-cdc)
transport/http.rs     → AsyncChunkTransport (reqwless, feature: http) — WiFi + Ethernet
transport/lora.rs     → ChunkTransport (blocking SPI, feature: lora) — SX1262/SX1276
ferrite-ble-nrf/      → AsyncChunkTransport (nrf-softdevice) — separate crate
```

## Deploy — RPi gateway (`deploy/rpi-gateway/`)
```bash
# Cross-compile for aarch64
bash deploy/rpi-gateway/cross-build.sh

# Deploy to RPi
scp -r deploy/rpi-gateway/ pi@raspberrypi:~/ferrite-deploy/
ssh pi@raspberrypi 'cd ~/ferrite-deploy && sudo bash setup.sh'
```

## Demo (`demo/`)
```bash
# Seed dashboard with 24h of demo data
python3 demo/seed_data.py --server http://localhost:4000 --hours 24

# Pre-flight check (run on RPi before demo)
bash demo/preflight.sh http://localhost:4000
```

## Module map — ferrite-sdk
```
memory.rs        → retained RAM layout + magic number validation
reboot_reason.rs → RebootReason type + retained RAM r/w
fault.rs         → HardFault handler + FaultRecord (cortex-m feature)
metrics.rs       → MetricsBuffer<N> ringbuffer
trace.rs         → TraceBuffer<N> circular log buffer
chunks/          → binary chunk encode/decode
transport.rs     → ChunkTransport trait + UART impl
upload.rs        → UploadManager orchestration
sdk.rs           → global state + init()
defmt_sink.rs    → defmt Logger impl (defmt feature)
```

## Module map — ferrite-server
```
main.rs           → AppState, CLI args, server startup
config.rs         → AuthConfig::from_env(), AuthMode enum, KeycloakConfig, BasicAuthConfig
auth.rs           → validate_basic_auth(), validate_keycloak_token(), validate_request()
auth_middleware.rs → require_auth() Axum middleware (path-based routing)
ingest.rs         → chunk decode, HTTP handlers, router with CORS
store.rs          → SQLite persistence (devices, faults, metrics, reboots)
symbolicator.rs   → ELF symbolication via addr2line
```

## Module map — ferrite-dashboard
```
main.rs           → App root, auth state init, route definitions, AppLayout guard
auth/state.rs     → AuthState, AuthToken, AuthModeInfo, UserInfo types
auth/mod.rs       → re-exports
api/client.rs     → ApiClient with auth headers, get_auth_mode()
api/types.rs      → Device, FaultEvent, MetricEntry API types
components/       → Navbar (with user info + logout)
pages/login.rs    → LoginPage (Keycloak redirect or Basic auth form)
pages/            → Dashboard, Devices, DeviceDetail, Faults, Metrics, Settings
```

## Critical embedded pitfalls (learned from fleet bring-up)
- **Embassy WFE wake bug**: Default embassy-executor thread-mode uses WFE which doesn't wake on STM32L4A6 without debugger. Use `raw::Executor` with spin-poll loop.
- **panic-probe without debugger**: Causes infinite reset loop. Use custom `#[panic_handler] fn panic(_) -> ! { loop { nop() } }` for standalone operation.
- **No FPU**: STM32WL55 CM4 and nRF5340 CM33 lack FPU. Must use `-eabi` (soft-float) target and avoid all float ops.
- **LPUART vs USART for VCP**: NUCLEO-WL55JC1 VCP routes through LPUART1 (AF8), not USART2 (AF7).
- **nRF5340 dual-core BLE**: Network core must be separately programmed with `hci_ipc` sample for BLE to work.
- **ESP ecosystem versions**: esp-hal 1.0.0 broke companion crates. Pin to 0.23 ecosystem.
- **USB CDC DTR**: Gateway must set DTR on serial port open for USB CDC devices to send.
- **Gateway chunk batching**: Without batching, individual chunk POSTs lose DeviceInfo context → "unknown" device.
- **Excluded workspace examples**: `.cargo/config.toml` rustflags may not apply. Use `build.rs` for linker args.
- **cbindgen naming**: `prefix_with_name = true` generates `FERRITE_ERROR_T_OK`, not `FERRITE_ERROR_OK`.
- **RDP/APPROTECT on new boards**: WL55 needs `st-flash --connect-under-reset erase`, nRF5340 needs `nrfjprog --recover`.

## Adding a new ChunkType
1. Add variant to ChunkType enum in `chunks/types.rs`
2. Add encode fn to ChunkEncoder in `chunks/encoder.rs`
3. Add decode match arm in ChunkDecoder in `chunks/decoder.rs`
4. Add SQL column in `ferrite-server/src/store.rs`
5. Add test in `chunks/encoder.rs` tests
