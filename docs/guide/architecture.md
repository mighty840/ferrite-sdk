# Architecture

This page describes the module structure of iotai-sdk, the data flow from firmware event to server storage, and how the crates in the workspace relate to each other.

## Module map

The core `iotai-sdk` crate is organized into the following modules:

```
iotai-sdk/src/
  lib.rs           SdkError enum, re-exports
  sdk.rs           Global SdkState, init(), with_sdk()
  config.rs        Compile-time constants (buffer sizes, max chunk size)
  memory.rs        RetainedBlock layout, retained! macro, magic validation
  reboot_reason.rs RebootReason enum, record/read/clear from retained RAM
  fault.rs         FaultRecord, ExceptionFrame, HardFault handler (cortex-m)
  metrics.rs       MetricsBuffer<N>, MetricEntry, metric macros
  trace.rs         TraceBuffer<N>, TraceFrame, circular byte buffer
  transport.rs     ChunkTransport / AsyncChunkTransport traits
  upload.rs        UploadManager: orchestrates a full upload session
  defmt_sink.rs    defmt Logger impl that writes to TraceBuffer (defmt feature)
  chunks/
    types.rs       ChunkType, ChunkHeader, DecodedChunk, DecodeError
    encoder.rs     ChunkEncoder: serializes SDK data into wire-format chunks
    decoder.rs     ChunkDecoder: validates and parses raw bytes into chunks
    mod.rs         Module re-exports
```

## Data flow

```
Firmware runtime                    Retained RAM               Upload path
+------------------+               +---------------+           +------------------+
| metric_gauge!()  |---> MetricsBuffer<32>          |           |                  |
| metric_increment!|    (in SdkState)               |           |                  |
| metric_observe!  |                                |           |                  |
+------------------+               +---------------+           |                  |
                                                               |  UploadManager   |
+------------------+               +---------------+           |  .upload()       |
| defmt::info!()   |---> TraceBuffer<512>           |           |                  |
| (defmt feature)  |    (in SdkState)               |     +---->  1. DeviceInfo   |
+------------------+               +---------------+     |     |  2. RebootReason |
                                                          |     |  3. FaultRecord  |
+------------------+               +---------------+     |     |  4. Metrics      |
| HardFault ISR    |---> FaultRecord                |-----+     |  5. Trace        |
| (cortex-m feat.) |    (in RetainedBlock)          |     |     |  6. Heartbeat    |
+------------------+               +---------------+     |     +--------+---------+
                                                          |              |
+------------------+               +---------------+     |              v
| record_reboot    |---> RebootReasonRecord         |-----+     ChunkTransport
|   _reason()      |    (in RetainedBlock)          |           .send_chunk()
+------------------+               +---------------+              |
                                                                  v
                                                          [UART / BLE / HTTP / ...]
                                                                  |
                                                                  v
                                                          iotai-server
                                                          POST /ingest/chunks
                                                                  |
                                                                  v
                                                          SQLite (devices, faults,
                                                                  metrics, reboots)
```

## Crate dependency graph

```
iotai-sdk  (core, no_std)
    |
    +--- iotai-sdk-embassy  (depends on iotai-sdk + embassy-*)
    |       Provides: iotai_upload_task, iotai_upload_task_with_trigger
    |
    +--- iotai-sdk-rtic  (depends on iotai-sdk)
    |       Provides: upload_blocking(), RticTransportResource
    |
    +--- iotai-sdk-ffi  (depends on iotai-sdk)
            Provides: C-callable functions, builds libiotai_sdk_ffi.a

iotai-server  (std, runs on host)
    Independent; re-implements chunk decoding for std
    Uses: axum, rusqlite, tokio, clap
```

## Critical section model

All access to `SdkState` goes through `sdk::with_sdk()`, which acquires a `critical_section::Mutex`. On Cortex-M hardware, this disables interrupts for the duration of the closure. On host (for testing), it uses a `std::sync::Mutex`.

The HardFault handler does **not** use the critical section -- it writes directly to retained RAM via raw pointer. This is safe because the fault handler runs at the highest exception priority and no other code can preempt it. The handler never returns; it writes the fault record and resets the processor.

## Feature flags

| Feature | Effect |
|---|---|
| `cortex-m` | Enables the HardFault handler, pulls in `cortex-m` and `cortex-m-rt` |
| `defmt` | Enables the `defmt_sink` module that captures defmt output into the TraceBuffer |
| `embassy` | Enables `AsyncChunkTransport` trait and async upload in `UploadManager` |
| `postcard` | Enables optional postcard-based serialization (alternative to hand-rolled encoding) |

For host testing, disable all features:

```bash
cargo test -p iotai-sdk --no-default-features
```

## Server architecture

The `iotai-server` binary is a standard Rust CLI application built with:

- **clap** for argument parsing (subcommands: `serve`, `report`, `faults`, `metrics`)
- **axum** for the HTTP server
- **rusqlite** for SQLite storage (WAL mode, foreign keys enabled)
- **tokio** for the async runtime

The server re-implements chunk decoding in `ingest.rs` (it does not depend on the `iotai-sdk` crate, since it needs `std`). It parses each chunk type's payload and stores the data in normalized SQLite tables.

### HTTP API endpoints

| Method | Path | Description |
|---|---|---|
| POST | `/ingest/chunks` | Accept binary chunk data (one or more concatenated chunks) |
| POST | `/ingest/elf` | Upload an ELF file for symbolication |
| GET | `/devices` | List all known devices |
| GET | `/devices/{id}/faults` | List fault events for a device |
| GET | `/devices/{id}/metrics` | List metric entries for a device |

### Symbolication

When a `FaultRecord` chunk arrives, the server attempts to resolve the PC address using `arm-none-eabi-addr2line`. ELF files are uploaded separately via `POST /ingest/elf` and stored in the configured `--elf-dir`. The resolved symbol (e.g., `main at src/main.rs:42`) is stored alongside the fault record in SQLite.
