# Core Concepts

This page explains the fundamental concepts behind iotai-sdk: how data survives reboots, how the fault capture lifecycle works, how telemetry is encoded into chunks, and how the transport abstraction decouples the SDK from your connectivity hardware.

## Retained RAM

Most Cortex-M microcontrollers do not clear SRAM on a soft reset (watchdog, software reset, or fault-triggered reset). Only a power-on reset or brownout zeroes memory. iotai-sdk exploits this by placing a `RetainedBlock` structure in a dedicated linker section (`.uninit.iotai`) that the C runtime startup code does **not** zero-initialize.

The retained block layout is:

```
RetainedBlock (256 bytes max, repr(C))
+------------------+
| RetainedHeader   |  12 bytes
|   magic: u32     |  0xABCD1234 when valid
|   sequence: u32  |  boot counter, incremented each init()
|   crc: u16       |  reserved for future header CRC
|   _pad: u16      |
+------------------+
| RebootReasonRec  |  16 bytes
|   magic: u32     |  0xBBCCDDEE when valid
|   reason: u8     |  RebootReason discriminant
|   extra: u8      |  user-defined (e.g. watchdog timer ID)
|   _pad: [u8; 10] |
+------------------+
| FaultRecord      |  ~168 bytes
|   valid: bool    |
|   fault_type: u8 |
|   frame (8xU32)  |  hardware exception frame
|   extended (9xU32)|  software-captured R4-R11 + SP
|   stack_snapshot  |  16 x u32 (64 bytes)
|   cfsr, hfsr,    |
|   mmfar, bfar    |  fault status registers
+------------------+
| metrics_dirty    |  1 byte + 3 padding
+------------------+
```

### Magic number validation

On each call to `init()`, the SDK checks whether `header.magic == 0xABCD1234`. If the magic is valid, the retained block contains data from the previous boot -- a fault record, a reboot reason, or both. The SDK preserves this data so it can be uploaded.

If the magic is invalid (first power-on, or RAM was corrupted), the SDK initializes the block to zeroes and writes the magic. Either way, the boot sequence counter is incremented.

### Linker script requirement

For retained RAM to work, you **must** reserve a region in your linker script and place the `.uninit.iotai` section into it. The section is marked `NOLOAD` so the startup code does not touch it. See the [Quickstart](./quickstart#step-2----add-the-linker-script-fragment) for the exact linker syntax.

The SDK provides pre-built linker fragments for common targets:

| Target | File | Retained address |
|---|---|---|
| nRF52840 | `linker/nrf52840-retained.x` | `0x20003F00` (end of 256KB SRAM) |
| RP2040 | `linker/rp2040-retained.x` | `0x20041F00` (end of 264KB SRAM) |
| STM32F4 | `linker/stm32f4-retained.x` | `0x2001FF00` (end of 128KB SRAM1) |

## Fault capture lifecycle

When a HardFault occurs on a Cortex-M processor, the hardware pushes an exception frame (R0-R3, R12, LR, PC, xPSR) onto the stack. The SDK's fault handler then:

1. **Captures extended registers** (R4-R11 and SP) using inline assembly. These are not part of the hardware-pushed frame but are essential for debugging.

2. **Takes a stack snapshot.** The handler reads the 16 words (64 bytes) above the stack pointer at fault time. Each address is validated against the registered RAM regions to avoid a secondary fault from reading an invalid address.

3. **Reads fault status registers.** CFSR (Configurable Fault Status Register), HFSR (HardFault Status Register), MMFAR (MemManage Fault Address Register), and BFAR (BusFault Address Register) tell you *why* the fault happened.

4. **Writes the complete `FaultRecord` to retained RAM.** The `valid` flag is set to `true`.

5. **Sets the reboot reason to `HardFault`** in the retained block.

6. **Triggers a system reset** via `SCB::sys_reset()`.

On the next boot, `init()` finds the valid fault record. The next upload session serializes it into a `FaultRecord` chunk and sends it to the server. After successful upload, the record is cleared.

### Fault types

The SDK classifies faults into four types, matching the Cortex-M exception architecture:

| Type | Value | Trigger |
|---|---|---|
| `HardFault` | 0 | Escalated fault or direct hard fault |
| `MemFault` | 1 | MPU violation (CFSR bits [7:0]) |
| `BusFault` | 2 | Bus error on instruction fetch or data access (CFSR bits [15:8]) |
| `UsageFault` | 3 | Undefined instruction, unaligned access, divide by zero (CFSR bits [25:16]) |

## Chunks

All data leaving the SDK is encoded as **chunks** -- self-contained binary messages with a fixed header, a variable-length payload, and a CRC-16 trailer. The maximum chunk size is 256 bytes.

### Why chunks?

Chunks are designed for unreliable, bandwidth-constrained transports like UART, BLE, or LoRa:

- **Self-framing.** Each chunk starts with a magic byte (`0xEC`) so the receiver can re-synchronize after a dropped byte.
- **Integrity checked.** CRC-16/CCITT-FALSE covers the header and payload. Corrupted chunks are detected and discarded.
- **Small.** A 256-byte maximum means a single chunk fits in one BLE ATT MTU (with room to spare) or one LoRa payload.
- **Ordered but independent.** Chunks carry a sequence number for ordering but each chunk is independently decodable. No reassembly state is required (except for trace fragments).

### Chunk types

| Type | Code | Description |
|---|---|---|
| Heartbeat | `0x01` | Uptime, free stack, metrics count, frames lost |
| Metrics | `0x02` | Batch of counter/gauge/histogram entries |
| FaultRecord | `0x03` | Full crash dump (registers + stack snapshot) |
| TraceFragment | `0x04` | Fragment of the trace buffer (with byte offset) |
| RebootReason | `0x05` | Why the device rebooted + boot sequence number |
| DeviceInfo | `0x06` | Device ID, firmware version, build ID |

See [Chunk Wire Format](../reference/chunk-format) for the complete byte-level specification.

### Upload session order

The `UploadManager` always sends chunks in a fixed order:

1. **DeviceInfo** (always first -- identifies the device)
2. **RebootReason** (if a valid record exists in retained RAM)
3. **FaultRecord** (if a valid fault was captured)
4. **Metrics** (all buffered entries, possibly spanning multiple chunks)
5. **TraceFragment** (all buffered trace data, fragmented)
6. **Heartbeat** (always last -- acts as a session-complete marker)

If the transport fails at any point, the session is aborted. Data that was not yet sent (or was sent but not acknowledged, depending on the transport) is retained for the next attempt. Data that was successfully sent is cleared from the buffers.

## Transport abstraction

The SDK defines a `ChunkTransport` trait for synchronous (blocking) transports and an `AsyncChunkTransport` trait for async transports:

```rust
pub trait ChunkTransport {
    type Error: core::fmt::Debug;

    /// Send a single encoded chunk (up to 256 bytes).
    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error>;

    /// Return true if the transport is ready to send.
    fn is_available(&self) -> bool { true }

    /// Called before a batch upload begins. Optional.
    fn begin_session(&mut self) -> Result<(), Self::Error> { Ok(()) }

    /// Called after a batch upload completes. Optional.
    fn end_session(&mut self) -> Result<(), Self::Error> { Ok(()) }
}
```

The async variant has the same methods but they are `async fn`. The C FFI uses function pointers with a `void* ctx` parameter instead.

### Session lifecycle

A transport session wraps a complete upload cycle:

1. `is_available()` -- SDK checks if the link is up before starting. If `false`, the upload is skipped and data is retained.
2. `begin_session()` -- Optional hook to power up a radio, open a TCP connection, or start a BLE notification stream.
3. `send_chunk()` -- Called once per chunk, in order. If this returns an error, the session is aborted.
4. `end_session()` -- Optional hook to tear down the connection, flush buffers, or power down the radio.

### Implementing a transport

To add a new transport, you only need to implement `send_chunk`. The other methods have sensible defaults. Here are some examples:

| Transport | `begin_session` | `send_chunk` | `end_session` |
|---|---|---|---|
| UART | no-op | write bytes to UART TX | no-op |
| BLE GATT | no-op (always connected) | write to notification characteristic | no-op |
| HTTP POST | no-op | buffer chunk | flush all buffered chunks as one POST body |
| LoRa | wake radio | send_chunk as LoRa packet | sleep radio |
| USB CDC | no-op | write to USB bulk endpoint | no-op |

## Metrics vs Trace

The SDK has two separate telemetry subsystems that serve different purposes:

### Metrics

Metrics are **structured key-value pairs** with three types:

- **Counter** (`u32`): monotonically increasing. Calls to `metric_increment!` accumulate. Good for event counts.
- **Gauge** (`f32`): last-write-wins. Calls to `metric_gauge!` overwrite the previous value. Good for current readings (temperature, voltage).
- **Histogram** (`f32`): tracks min, max, sum, and count. Calls to `metric_observe!` update the running statistics. Good for latency distributions.

Metrics are stored in a `MetricsBuffer<N>` where `N` is the maximum number of distinct keys. The default is 32. Each key is a `heapless::String<32>` (max 32 UTF-8 bytes). When a new key arrives and the buffer is full, the oldest entry (by insertion order) is evicted.

Metrics are uploaded as `Metrics` chunks. Multiple entries are packed into a single chunk payload. If the entries do not fit in one chunk (248 bytes max payload), they are split across multiple chunks.

### Trace

Trace is an **unstructured binary log stream**. It captures raw defmt frames (or any byte payload you write to it) in a circular `TraceBuffer<N>` where `N` is the buffer capacity in bytes (default 512).

Each trace frame has the format:

```
[level: 1 byte][ticks_lo: 4 bytes LE][len: 1 byte][payload: len bytes][0xFF sentinel]
```

When the buffer fills, the oldest frames are evicted to make room. The `frames_lost` counter tracks how many frames were dropped.

Trace data is uploaded as `TraceFragment` chunks. Each fragment carries an 8-byte byte-offset prefix followed by the raw trace data. The server reassembles fragments in order using the byte offsets.

### When to use which

| Need | Use |
|---|---|
| Track how many times an event occurs | Counter metric |
| Monitor a current sensor reading | Gauge metric |
| Measure latency or duration distribution | Histogram metric |
| Debug log messages for post-mortem analysis | Trace |
| Understand control flow leading to a crash | Trace + Fault capture |

In practice, most applications use both: metrics for operational monitoring and trace for debugging. The upload session sends both in the same cycle.
