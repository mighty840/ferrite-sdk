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

## Flash examples (requires probe-rs)
```bash
cd examples/embassy-nrf52840
cargo run --release
```

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

## Adding a new ChunkType
1. Add variant to ChunkType enum in `chunks/types.rs`
2. Add encode fn to ChunkEncoder in `chunks/encoder.rs`
3. Add decode match arm in ChunkDecoder in `chunks/decoder.rs`
4. Add SQL column in `ferrite-server/src/store.rs`
5. Add test in `chunks/encoder.rs` tests
