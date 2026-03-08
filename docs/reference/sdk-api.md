# Rust SDK API Reference

This page documents all public types, functions, traits, and macros exported by the `iotai-sdk` crate.

## Crate root (`iotai_sdk`)

### `init(config: SdkConfig<'static>)`

Initialize the SDK. Must be called exactly once at firmware startup before any other SDK function.

**Panics** if called more than once.

**Actions:**
- Registers RAM regions for fault handler safety checks
- Validates the retained RAM block (initializes if magic is invalid)
- Increments the boot sequence counter
- Stores the `ticks_fn` globally
- Creates the global `SdkState` with a `MetricsBuffer<32>` and `TraceBuffer<512>`

### `is_initialized() -> bool`

Returns `true` if `init()` has been called.

### `with_sdk<F, R>(f: F) -> R`

Access the global SDK state inside a critical section. Panics if the SDK is not initialized.

```rust
iotai_sdk::with_sdk(|state| {
    // state: &mut SdkState
    state.metrics.len()
});
```

### `SdkError`

```rust
pub enum SdkError {
    NotInitialized,
    AlreadyInitialized,
    BufferFull,
    KeyTooLong,          // metric key > 32 bytes
    TooManyRamRegions,   // more than 4 regions
    InvalidConfig,
    EncodingFailed,
}
```

All public fallible functions return `Result<T, SdkError>`.

### `SdkConfig<'a>`

```rust
pub struct SdkConfig<'a> {
    pub device_id: &'a str,
    pub firmware_version: &'a str,
    pub build_id: u64,
    pub ticks_fn: fn() -> u64,
    pub ram_regions: &'a [RamRegion],
}
```

See [SdkConfig reference](./config) for details on each field.

### `RamRegion`

```rust
pub struct RamRegion {
    pub start: u32,  // inclusive
    pub end: u32,    // exclusive
}
```

---

## Macros

### `metric_increment!(key)` / `metric_increment!(key, delta)`

Increment a counter metric. The one-argument form increments by 1.

```rust
iotai_sdk::metric_increment!("packets_sent");
iotai_sdk::metric_increment!("bytes_sent", 1024);
```

Returns `Result<(), SdkError>`.

### `metric_gauge!(key, value)`

Set a gauge metric. `value` is cast to `f32`.

```rust
iotai_sdk::metric_gauge!("temperature", 23.5);
```

### `metric_observe!(key, value)`

Record a histogram observation. `value` is cast to `f32`. Tracks min, max, sum, and count.

```rust
iotai_sdk::metric_observe!("latency_ms", 12.5);
```

### `retained!(vis static NAME: Type = default)`

Place a static variable in the `.uninit.iotai` linker section (retained RAM).

```rust
iotai_sdk::retained!(pub static MY_DATA: u32 = 0);
```

---

## `metrics` module

### `MetricsBuffer<const N: usize>`

Fixed-capacity buffer for metric entries. `N` is the maximum number of distinct keys.

| Method | Description |
|---|---|
| `new() -> Self` | Create an empty buffer |
| `increment(key, delta, ticks) -> Result<(), SdkError>` | Add to a counter |
| `gauge(key, value, ticks) -> Result<(), SdkError>` | Set a gauge |
| `observe(key, value, ticks) -> Result<(), SdkError>` | Record histogram observation |
| `iter() -> impl Iterator<Item = &MetricEntry>` | Iterate all entries |
| `clear()` | Remove all entries |
| `len() -> usize` | Number of entries |
| `is_empty() -> bool` | Whether buffer is empty |

When the buffer is full and a new key is recorded, the oldest entry (index 0) is evicted.

### `MetricEntry`

```rust
pub struct MetricEntry {
    pub key: heapless::String<32>,
    pub value: MetricValue,
    pub timestamp_ticks: u64,
}
```

### `MetricValue`

```rust
pub enum MetricValue {
    Counter(u32),
    Gauge(f32),
    Histogram { min: f32, max: f32, sum: f32, count: u32 },
}
```

### `MetricType`

```rust
#[repr(u8)]
pub enum MetricType {
    Counter = 0,
    Gauge = 1,
    Histogram = 2,
}
```

### `ticks() -> u64`

Returns the current tick count from the registered `ticks_fn`.

---

## `trace` module

### `TraceBuffer<const N: usize>`

Circular byte buffer for structured trace frames.

| Method | Description |
|---|---|
| `new() -> Self` | Create an empty buffer |
| `write_frame(level, ticks, payload)` | Write a log frame; evicts oldest if full |
| `iter_frames() -> TraceFrameIter` | Iterate frames oldest to newest |
| `total_written() -> u64` | Lifetime byte count (including overwritten) |
| `frames_lost() -> u32` | Frames evicted due to overflow |
| `bytes_used() -> usize` | Currently occupied bytes |
| `clear()` | Reset read/write positions |

### `TraceFrame<'a>`

```rust
pub struct TraceFrame<'a> {
    pub level: u8,
    pub ticks: u32,
    pub payload: &'a [u8],
}
```

---

## `fault` module

### `FaultRecord`

```rust
#[repr(C)]
pub struct FaultRecord {
    pub valid: bool,
    pub fault_type: FaultType,
    pub frame: ExceptionFrame,
    pub extended: ExtendedRegisters,
    pub stack_snapshot: [u32; 16],
    pub cfsr: u32,
    pub hfsr: u32,
    pub mmfar: u32,
    pub bfar: u32,
}
```

### `FaultType`

```rust
#[repr(u8)]
pub enum FaultType {
    HardFault = 0,
    MemFault = 1,
    BusFault = 2,
    UsageFault = 3,
}
```

### `ExceptionFrame`

Hardware-pushed exception frame: `r0, r1, r2, r3, r12, lr, pc, xpsr` (all `u32`).

### `ExtendedRegisters`

Software-captured registers: `r4, r5, r6, r7, r8, r9, r10, r11, sp` (all `u32`).

### `last_fault() -> Option<FaultRecord>`

Read the fault record from the previous boot. Returns `None` if no valid fault is present.

### `clear_fault_record()`

Clear the fault record. Called automatically after a successful upload.

### `register_ram_region(start, end) -> Result<(), SdkError>`

Register a valid RAM address range. Up to 4 regions can be registered.

### `is_valid_ram_address(addr: u32) -> bool`

Check if an address falls within a registered RAM region.

---

## `reboot_reason` module

### `RebootReason`

```rust
#[repr(u8)]
pub enum RebootReason {
    Unknown = 0,
    PowerOnReset = 1,
    SoftwareReset = 2,
    WatchdogTimeout = 3,
    HardFault = 4,
    MemoryFault = 5,
    BusFault = 6,
    UsageFault = 7,
    AssertFailed = 8,
    PinReset = 9,
    BrownoutReset = 10,
    FirmwareUpdate = 11,
    UserRequested = 12,
}
```

### `record_reboot_reason(reason: RebootReason)`

Store the reboot reason in retained RAM.

### `record_reboot_reason_with_extra(reason, extra: u8)`

Store the reboot reason with an additional byte (e.g., watchdog timer ID).

### `last_reboot_reason() -> Option<RebootReason>`

Read the reboot reason from the previous boot. Returns `None` if no valid record exists.

### `clear_reboot_reason()`

Clear the record. Called automatically after a successful upload.

---

## `transport` module

### `ChunkTransport` trait

Blocking transport for sending encoded chunks.

```rust
pub trait ChunkTransport {
    type Error: core::fmt::Debug;
    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error>;
    fn is_available(&self) -> bool { true }
    fn begin_session(&mut self) -> Result<(), Self::Error> { Ok(()) }
    fn end_session(&mut self) -> Result<(), Self::Error> { Ok(()) }
}
```

### `AsyncChunkTransport` trait (embassy feature)

Async variant for Embassy. Same methods, but `async fn`.

### `UartTransport<UART>`

Generic UART wrapper. Provides `new(uart)` and `into_inner()`.

---

## `upload` module

### `UploadManager`

Orchestrates a complete upload session.

#### `upload<T: ChunkTransport>(transport: &mut T) -> Result<UploadStats, UploadError<T::Error>>`

Run a full blocking upload. Sends chunks in order: DeviceInfo, RebootReason, FaultRecord, Metrics, Trace, Heartbeat.

#### `upload_async<T: AsyncChunkTransport>(transport: &mut T) -> Result<UploadStats, UploadError<T::Error>>` (embassy feature)

Async variant. Collects chunks in a critical section, then sends them outside the critical section.

### `UploadStats`

```rust
pub struct UploadStats {
    pub chunks_sent: u32,
    pub bytes_sent: u32,
    pub fault_uploaded: bool,
    pub metrics_uploaded: u32,
    pub trace_bytes_uploaded: u32,
}
```

### `UploadError<E>`

```rust
pub enum UploadError<E> {
    TransportUnavailable,
    TransportError(E),
    EncodingError,
    NotInitialized,
}
```

---

## `chunks` module

### `ChunkType`

```rust
#[repr(u8)]
pub enum ChunkType {
    Heartbeat = 0x01,
    Metrics = 0x02,
    FaultRecord = 0x03,
    TraceFragment = 0x04,
    RebootReason = 0x05,
    DeviceInfo = 0x06,
}
```

### `ChunkHeader`

```rust
pub struct ChunkHeader {
    pub magic: u8,        // 0xEC
    pub version: u8,      // 1
    pub chunk_type: ChunkType,
    pub flags: u8,
    pub payload_len: u16,
    pub sequence_id: u16,
}
```

Constants: `MAGIC = 0xEC`, `VERSION = 1`, `WIRE_SIZE = 8`.

### `ChunkEncoder`

Encodes SDK data into wire-format chunks. Methods: `encode`, `encode_fault`, `encode_metrics`, `encode_trace`, `encode_heartbeat`, `encode_reboot_reason`, `encode_device_info`.

### `ChunkDecoder`

Validates and parses raw bytes. `decode(bytes) -> Result<DecodedChunk, DecodeError>`.

### `DecodedChunk`

```rust
pub struct DecodedChunk {
    pub chunk_type: ChunkType,
    pub sequence_id: u16,
    pub is_last: bool,
    pub payload: heapless::Vec<u8, 248>,
}
```
