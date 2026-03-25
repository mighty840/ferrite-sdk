# Contributing to ferrite-sdk

Thanks for your interest in contributing! This guide covers everything you need to get a local development environment running and start submitting changes.

## Prerequisites

You need **one** of the following installed:

| Tool | Purpose |
|------|---------|
| [Nix](https://nixos.org/download/) (recommended) | Single command setup — provides all toolchains, targets, and tools |
| [Rust](https://rustup.rs/) (manual) | If you prefer managing toolchains yourself |

### Option A: Nix (recommended)

Install Nix with flakes enabled:

```bash
# Install Nix (multi-user, recommended)
sh <(curl -L https://nixos.org/nix/install) --daemon

# Enable flakes (add to ~/.config/nix/nix.conf)
echo "experimental-features = nix-command flakes" >> ~/.config/nix/nix.conf
```

Then enter the dev environment:

```bash
cd ferrite-sdk
nix develop
```

This gives you **everything**: stable Rust with all 7 embedded targets + WASM, probe-rs, dx CLI, cbindgen, arm-none-eabi-gcc, cross, QEMU, Node.js for docs, and more. No further setup needed.

Optional: install [direnv](https://direnv.net/) for automatic shell activation:

```bash
# Install direnv and hook it into your shell
nix profile install nixpkgs#direnv
echo 'eval "$(direnv hook bash)"' >> ~/.bashrc  # or zsh

# Allow the project's .envrc
cd ferrite-sdk
direnv allow
```

Now the dev environment activates automatically when you `cd` into the project.

#### ESP32-C3 development

The ESP32-C3 needs a pinned nightly toolchain (2025-04-15). Use the separate shell:

```bash
nix develop .#esp
cd examples/embassy-esp32c3
cargo build --release
```

### Option B: Manual setup

If you prefer not to use Nix, install these manually:

```bash
# 1. Rust stable + embedded targets
rustup default stable
rustup target add thumbv6m-none-eabi          # RP2040
rustup target add thumbv7m-none-eabi          # Cortex-M3
rustup target add thumbv7em-none-eabi         # STM32WL55 (no FPU)
rustup target add thumbv7em-none-eabihf       # STM32L4A6, nRF52840
rustup target add thumbv8m.main-none-eabi     # nRF5340 (no FPU)
rustup target add thumbv8m.main-none-eabihf   # STM32H563
rustup target add wasm32-unknown-unknown      # Dashboard
rustup component add clippy rustfmt rust-analyzer

# 2. Embedded tools
cargo install probe-rs-tools    # Flash & debug
cargo install cargo-binutils    # objcopy, nm, size
cargo install cbindgen          # C FFI headers
cargo install cross             # RPi cross-compilation
cargo install dioxus-cli@0.6.3  # Dashboard dev server

# 3. System packages (Debian/Ubuntu)
sudo apt install pkg-config libssl-dev libudev-dev libdbus-1-dev \
    gcc-arm-none-eabi qemu-system-arm nodejs npm

# 4. ESP32-C3 (optional, needs nightly)
rustup toolchain install nightly-2025-04-15
rustup target add riscv32imc-unknown-none-elf --toolchain nightly-2025-04-15
pip install esptool
```

## Building and testing

### Host tests (no hardware needed)

```bash
# SDK tests (single-threaded — global state requires it)
cargo test -p ferrite-sdk --no-default-features -- --test-threads=1

# Server tests
cargo test -p ferrite-server

# Formatting and linting
cargo fmt --all -- --check
cargo clippy -p ferrite-sdk --no-default-features -- -D warnings
cargo clippy -p ferrite-server -- -D warnings
```

### Embedded builds

```bash
# Build SDK for Cortex-M4F
cargo build -p ferrite-sdk --features cortex-m,defmt,embassy --target thumbv7em-none-eabihf

# Build a board example
cargo build --manifest-path examples/embassy-nrf52840/Cargo.toml --target thumbv7em-none-eabihf

# QEMU integration tests (lm3s6965evb)
rustup target add thumbv7m-none-eabi
cd tests/qemu && cargo run --release
```

### Dashboard

```bash
# Terminal 1: start the server
cargo run -p ferrite-server

# Terminal 2: start the dashboard dev server (proxies API to localhost:4000)
cd ferrite-dashboard && dx serve
```

The dashboard opens at `http://localhost:8080`. Default login: `admin` / `admin`.

### Docs site

```bash
cd docs
npm install
npm run dev       # Dev server at localhost:5173
npm run build     # Production build
```

## Project structure

```
ferrite-sdk/          Core no_std SDK (crashes, metrics, trace, chunks)
ferrite-server/       Axum ingestion server (auth, alerting, Prometheus)
ferrite-dashboard/    Dioxus WASM dashboard (fleet monitoring UI)
ferrite-gateway/      Edge gateway (BLE/USB/LoRa → server bridge)
ferrite-embassy/      Embassy async upload task
ferrite-rtic/         RTIC blocking upload wrapper
ferrite-ffi/          C FFI static library (Zephyr/FreeRTOS)
ferrite-provision/    Device provisioning CLI
examples/             Board-specific firmware examples
tests/qemu/           QEMU integration tests (excluded from workspace)
docs/                 VitePress documentation site
deploy/               Deployment configs (RPi gateway, Docker)
```

## Making changes

### Workflow

1. Fork the repo and create a feature branch from `main`
2. Make your changes
3. Run the relevant tests (see above)
4. Ensure `cargo fmt --all` and `cargo clippy` pass
5. Open a pull request against `main`

### Commit style

We use conventional commits:

```
feat: add BLE transport for nRF5340
fix: correct CRC calculation for chunks > 255 bytes
docs: add gateway deployment guide
refactor: extract chunk encoding into separate module
```

### Key design constraints

These are non-negotiable for the SDK:

- **No alloc** — `ferrite-sdk` must remain `#![no_std]` with no allocator
- **No panics** — production code paths must never panic (tests are fine)
- **Feature-gated hardware** — all MCU-specific code behind feature flags
- **Global state via `CriticalSectionMutex`** — tests must run single-threaded
- **CRC-16/CCITT-FALSE** — chunk integrity uses this specific CRC variant

### Adding a new ChunkType

1. Add variant to `ChunkType` enum in `ferrite-sdk/src/chunks/types.rs`
2. Add encode function in `ferrite-sdk/src/chunks/encoder.rs`
3. Add decode match arm in `ferrite-sdk/src/chunks/decoder.rs`
4. Add SQL column in `ferrite-server/src/store.rs`
5. Add test in `ferrite-sdk/src/chunks/encoder.rs`

### Adding a new transport

1. Create `ferrite-sdk/src/transport/my_transport.rs`
2. Implement `ChunkTransport` (blocking) or `AsyncChunkTransport` (async)
3. Gate behind a feature flag in `ferrite-sdk/Cargo.toml`
4. Add gateway support in `ferrite-gateway/` if bridging is needed
5. Document in `docs/guide/transports.md`

### Dashboard changes

The dashboard uses [Dioxus 0.7](https://dioxuslabs.com/) with Tailwind CSS. Key patterns:

- State: `use_context_provider()` to provide, `use_context::<Signal<T>>()` to consume
- Auth: discovered at startup via `GET /auth/mode`
- API client: `src/api/client.rs` — all requests go through `ApiClient`
- Routing: defined in `src/main.rs`, guarded by `AppLayout`

### Server changes

The server uses [Axum](https://github.com/tokio-rs/axum) with SQLite (via rusqlite). Key patterns:

- Auth middleware in `src/auth_middleware.rs` — must pass `OPTIONS` for CORS
- Layer order matters: auth middleware before CORS layer
- Config: `AuthConfig::from_env()` with `Box::leak` for `&'static` lifetime
- All new endpoints need entries in `src/main.rs` router and CI proxy config

## Flashing firmware to boards

If you have physical dev boards:

```bash
# STM32 boards (probe-rs)
probe-rs download --chip STM32L4A6ZGTx target/thumbv7em-none-eabihf/release/binary
probe-rs reset --chip STM32L4A6ZGTx

# STM32WL55 (st-flash — probe-rs can't connect without reset)
arm-none-eabi-objcopy -O binary target/.../release/binary /tmp/fw.bin
st-flash --connect-under-reset write /tmp/fw.bin 0x8000000

# ESP32-C3
espflash flash --chip esp32c3 target/riscv32imc-unknown-none-elf/release/binary

# nRF5340 (J-Link)
nrfjprog --program build/zephyr/zephyr.hex -f NRF53 --coprocessor CP_APPLICATION --sectorerase --reset
```

See `CLAUDE.md` for detailed flash instructions and common pitfalls.

## CI

Every PR runs:

| Job | What it checks |
|-----|----------------|
| Host tests | `cargo fmt`, `clippy`, SDK tests, server tests |
| Keycloak tests | Server integration with real Keycloak container |
| Embedded builds | SDK compiles for nRF52840, RP2040, STM32F4 targets |
| QEMU tests | Integration tests on emulated Cortex-M3 |

The deploy job triggers a Coolify webhook on merge to `main`.

## Questions?

Open an issue at [github.com/mighty840/ferrite-sdk/issues](https://github.com/mighty840/ferrite-sdk/issues).
