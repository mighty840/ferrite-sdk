# iotai-sdk — Build Guide for AI Coding Agents

## Build (host, no embedded toolchain needed)
```bash
cargo build -p iotai-sdk --no-default-features
cargo test -p iotai-sdk --no-default-features
cargo test -p iotai-server
```

## Build for embedded (requires ARM toolchain)
```bash
rustup target add thumbv7em-none-eabihf
cargo build -p iotai-sdk --features cortex-m,defmt,embassy --target thumbv7em-none-eabihf
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

## Key design decisions
- No alloc anywhere in iotai-sdk core
- No panics in production code paths (tests excepted)
- Feature flags gate all hardware dependencies
- cortex-m feature must be disabled for host tests
- `critical-section` crate for portable critical sections (enables host testing)
- Global state via `CriticalSectionMutex<RefCell<Option<SdkState>>>`
- Hand-rolled binary chunk encoding (postcard optional via feature)
- CRC-16/CCITT-FALSE for chunk integrity

## Module map
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

## Adding a new ChunkType
1. Add variant to ChunkType enum in `chunks/types.rs`
2. Add encode fn to ChunkEncoder in `chunks/encoder.rs`
3. Add decode match arm in ChunkDecoder in `chunks/decoder.rs`
4. Add SQL column in `iotai-server/src/store.rs`
5. Add test in `chunks/encoder.rs` tests
