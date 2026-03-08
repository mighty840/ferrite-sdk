# Testing

This page describes how to run tests for each crate in the workspace and the testing strategy.

## Host tests

Most SDK logic can be tested on the host without any embedded hardware. The key is to disable the `cortex-m` feature:

```bash
# Core SDK tests
cargo test -p iotai-sdk --no-default-features

# Server tests
cargo test -p iotai-server
```

The `--no-default-features` flag disables `cortex-m`, `defmt`, and `embassy`, allowing the SDK to compile and run on the host. The `critical-section` crate provides a `std`-based implementation for host testing.

### What is tested on the host

- **MetricsBuffer**: counter accumulation, gauge overwrite, histogram aggregation, buffer full eviction, key length validation, clear, iteration
- **TraceBuffer**: write and iterate, circular wrap-around, frame eviction, frames_lost counter, clear, total_written accumulation
- **ChunkEncoder/Decoder**: encode-decode roundtrip for all chunk types, CRC validation, CRC mismatch detection, sequence number wrapping, oversized payload truncation
- **FaultRecord**: serialization size, roundtrip encode/decode
- **RebootReason**: record/read roundtrip, clear, from_u8 conversion
- **UploadManager**: upload order (DeviceInfo first, Heartbeat last), transport unavailable handling, buffer clearing on success
- **Retained RAM**: magic number validation, zeroed initialization

### Running a single test

```bash
cargo test -p iotai-sdk --no-default-features -- counter_accumulation
```

### Viewing test output

```bash
cargo test -p iotai-sdk --no-default-features -- --nocapture
```

## Embedded tests (QEMU)

The repository includes QEMU-based tests that run on an emulated Cortex-M3:

```bash
rustup target add thumbv7m-none-eabi
cd tests/qemu
cargo run --release
```

These tests verify that the SDK works correctly in a real `no_std` environment with interrupts and actual critical sections.

## Server tests

The server has unit tests for:

- SQLite store operations (using in-memory databases)
- CRC-16 computation (known test vectors)
- Chunk decoding (valid chunks, bad magic, bad version, CRC mismatch, truncated payload)
- Payload parsing (DeviceInfo, Metrics, FaultRecord, RebootReason, Heartbeat)
- Multi-chunk stream decoding

```bash
cargo test -p iotai-server
```

## Test coverage

The project aims for high test coverage of core logic. Areas that are difficult to test on the host include:

- The actual HardFault handler (requires real Cortex-M hardware or QEMU with fault injection)
- Async upload via `AsyncChunkTransport` (requires Embassy runtime)
- RTIC integration (requires RTIC runtime)
- C FFI functions (tested implicitly through the Rust SDK tests)

## Writing new tests

When adding tests:

1. Place tests in the same file as the code being tested, inside a `#[cfg(test)] mod tests` block.
2. Use `extern crate std;` at the top of the test module (needed because the crate is `no_std`).
3. Avoid depending on global state order between tests -- the SDK's global state (`INITIALIZED`, `SDK`, `RAM_REGIONS`) is shared across tests in the same process.
4. For tests that need the SDK initialized, check `is_initialized()` first and only call `init()` if it has not been called yet.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    fn ensure_init() {
        if !crate::sdk::is_initialized() {
            crate::init(crate::SdkConfig {
                device_id: "test",
                firmware_version: "0.0.0",
                build_id: 0,
                ticks_fn: || 0,
                ram_regions: &[],
            });
        }
    }

    #[test]
    fn my_test() {
        ensure_init();
        // ... test logic ...
    }
}
```
