# SdkConfig Reference

This page documents all fields in `SdkConfig` and the compile-time constants that control buffer sizes and chunk limits.

## SdkConfig fields

```rust
pub struct SdkConfig<'a> {
    pub device_id: &'a str,
    pub firmware_version: &'a str,
    pub build_id: u64,
    pub ticks_fn: fn() -> u64,
    pub ram_regions: &'a [RamRegion],
}
```

### `device_id: &'a str`

A unique identifier for this device. Sent in the DeviceInfo chunk at the start of every upload session. The server uses this to group telemetry by device.

- Maximum length: 127 bytes (truncated silently if longer)
- Must point to memory that lives for `'static` (typically a string literal)
- Example: `"sensor-42"`, `"gateway-us-east-01"`

### `firmware_version: &'a str`

The firmware version string. Used by the server for symbolication (matching ELF files) and for tracking which firmware version a device is running.

- Maximum length: 127 bytes
- Must point to `'static` memory
- Common pattern: `env!("CARGO_PKG_VERSION")` to use the version from `Cargo.toml`

### `build_id: u64`

The first 8 bytes of the ELF `.build_id` section, interpreted as a `u64`. This provides a unique identifier for the exact binary, even across identical version strings.

Pass 0 if you do not embed a build ID in your ELF.

To extract the build ID at compile time, you can use a build script:

```rust
// build.rs
fn main() {
    // Set a build ID based on the git hash or a random value
    println!("cargo:rustc-env=BUILD_ID=0x{:016X}", generate_build_id());
}
```

### `ticks_fn: fn() -> u64`

A function pointer that returns the current monotonic tick count. Called every time a metric is recorded or a trace frame is written. The returned value is stored alongside each metric entry and trace frame for timestamp ordering.

Requirements:
- Must be monotonically non-decreasing
- Must not panic
- Should be cheap to call (no syscalls, no blocking)
- The absolute value does not matter -- only relative ordering is used

Common implementations:

| Framework | Implementation |
|---|---|
| Embassy | `\|\| embassy_time::Instant::now().as_ticks()` |
| RTIC | `\|\| rtic_monotonics::systick::Systick::now().ticks()` |
| Bare-metal | `\|\| read_systick_counter()` or DWT cycle counter |
| Zephyr (C) | `k_uptime_ticks()` |
| FreeRTOS (C) | `xTaskGetTickCount()` |

### `ram_regions: &'a [RamRegion]`

An array of valid RAM address ranges. The HardFault handler uses these to validate addresses before reading stack memory, preventing a secondary fault from an invalid pointer dereference.

- Maximum 4 regions (returns `SdkError::TooManyRamRegions` if exceeded)
- Each region is `[start, end)` (start inclusive, end exclusive)
- Pass `&[]` if you do not want stack snapshot validation (the fault handler will still capture registers but skip the stack snapshot)

For most single-SRAM-bank microcontrollers, one region covering the entire SRAM is sufficient:

```rust
ram_regions: &[RamRegion {
    start: 0x2000_0000,
    end: 0x2004_0000,
}],
```

For MCUs with multiple SRAM banks (e.g., STM32F407 with CCM):

```rust
ram_regions: &[
    RamRegion { start: 0x2000_0000, end: 0x2002_0000 }, // 128 KB SRAM1
    RamRegion { start: 0x1000_0000, end: 0x1001_0000 }, // 64 KB CCM
],
```

## Compile-time constants

These constants are defined in `iotai-sdk/src/config.rs` and control buffer sizes and chunk limits:

### `DEFAULT_METRICS_CAPACITY`

```rust
pub const DEFAULT_METRICS_CAPACITY: usize = 32;
```

Default number of metric entries in the `MetricsBuffer`. This is the `N` in `MetricsBuffer<N>` when using the global SDK state. Each entry stores a key (up to 32 bytes), a value (4-16 bytes), and a timestamp (8 bytes).

To use a different capacity, you would need to modify the `SdkState` definition in `sdk.rs`.

### `DEFAULT_TRACE_BUFFER_SIZE`

```rust
pub const DEFAULT_TRACE_BUFFER_SIZE: usize = 512;
```

Default capacity of the `TraceBuffer` in bytes. This is the `N` in `TraceBuffer<N>`. Each trace frame uses `6 + payload_len + 1` bytes, so a 512-byte buffer holds approximately 40-50 short log messages.

### `MAX_CHUNK_SIZE`

```rust
pub const MAX_CHUNK_SIZE: usize = 256;
```

Maximum size of an encoded chunk including header, payload, and CRC. This is the buffer size passed to `ChunkEncoder::encode()`.

### `MAX_PAYLOAD_SIZE`

```rust
pub const MAX_PAYLOAD_SIZE: usize = 248;
```

Maximum payload size per chunk. The total chunk overhead is 10 bytes (8-byte header + 2-byte CRC), so `MAX_PAYLOAD_SIZE = MAX_CHUNK_SIZE - 10 + 2 = 248`. (The actual implementation allows up to `256 - 10 = 246` bytes, with 248 being the configured limit.)

## Memory impact

| Constant | Effect on RAM | Effect on flash |
|---|---|---|
| `METRICS_CAPACITY = 32` | ~1.4 KB (32 entries x ~44 bytes each) | Negligible |
| `TRACE_BUFFER_SIZE = 512` | 512 bytes | Negligible |
| `MAX_CHUNK_SIZE = 256` | 256 bytes (encoder temp buffer, on stack) | Negligible |

The retained RAM block is always 256 bytes regardless of these constants.
