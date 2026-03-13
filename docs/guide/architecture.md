# Architecture

This page describes the module structure of ferrite-sdk, the data flow from firmware event to server storage, and how the crates in the workspace relate to each other.

## Module map

The core `ferrite-sdk` crate is organized into the following modules:

```
ferrite-sdk/src/
  lib.rs           SdkError enum, re-exports
  sdk.rs           Global SdkState, init(), with_sdk()
  config.rs        Compile-time constants (buffer sizes, max chunk size)
  memory.rs        RetainedBlock layout, retained! macro, magic validation
  reboot_reason.rs RebootReason enum, record/read/clear from retained RAM
  fault.rs         FaultRecord, ExceptionFrame, HardFault handler (cortex-m)
  metrics.rs       MetricsBuffer<N>, MetricEntry, metric macros
  trace.rs         TraceBuffer<N>, TraceFrame, circular byte buffer
  encryption.rs    AES-128-CCM EncryptedTransport wrapper
  compression.rs   RLE CompressedTransport wrapper + decompress utilities
  transport/
    mod.rs         Module re-exports, feature-gated transports
    traits.rs      ChunkTransport / AsyncChunkTransport traits
    uart.rs        UartTransport<UART> (always available)
    usb_cdc.rs     USB CDC via embassy-usb (usb-cdc feature)
    http.rs        WiFi/HTTP via reqwless (http feature)
    lora.rs        LoRa SX1262/SX1276 via embedded-hal SPI (lora feature)
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
                                              [Optional: EncryptedTransport / CompressedTransport]
                                                                  |
                                                                  v
                                                          [UART / BLE / USB CDC / HTTP / LoRa]
                                                                  |
                                                 +----------------+----------------+
                                                 |                                 |
                                           Direct to server              Via ferrite-gateway
                                                 |                    (BLE/USB bridge + buffer)
                                                 v                                 |
                                          ferrite-server                           v
                                          POST /ingest/chunks          ferrite-server
                                                 |                     POST /ingest/chunks
                                                 v
                                          SQLite (devices, faults,
                                                  metrics, reboots,
                                                  groups, ota_targets)
                                                 |
                                    +------------+-------------+
                                    |            |             |
                               Dashboard    Prometheus    Webhooks
                              (Dioxus WASM) (/metrics)   (alerting)
```

## Crate dependency graph

```
ferrite-sdk  (core, no_std)
    |
    +--- ferrite-embassy  (depends on ferrite-sdk + embassy-*)
    |       Provides: ferrite_upload_task, ferrite_upload_task_with_trigger
    |
    +--- ferrite-rtic  (depends on ferrite-sdk)
    |       Provides: upload_blocking(), RticTransportResource
    |
    +--- ferrite-ffi  (depends on ferrite-sdk)
    |       Provides: C-callable functions, builds libferrite_ffi.a
    |
    +--- ferrite-ble-nrf  (depends on ferrite-sdk, excluded from workspace)
            Provides: BLE GATT service UUIDs, nRF SoftDevice transport

ferrite-server  (std, runs on host)
    Independent; re-implements chunk decoding for std
    Uses: axum, rusqlite, tokio, clap, reqwest, aes-gcm, jsonwebtoken

ferrite-gateway  (std, runs on edge host)
    Independent; uses ChunkFramer for stream decoding
    Uses: tokio, clap, reqwest, rusqlite, serialport, btleplug

ferrite-dashboard  (Dioxus 0.7, compiles to WASM)
    Independent; consumes ferrite-server REST API
    Uses: dioxus (web + router), reqwest, gloo-timers
```

## Critical section model

All access to `SdkState` goes through `sdk::with_sdk()`, which acquires a `critical_section::Mutex`. On Cortex-M hardware, this disables interrupts for the duration of the closure. On host (for testing), it uses a `std::sync::Mutex`.

The HardFault handler does **not** use the critical section -- it writes directly to retained RAM via raw pointer. This is safe because the fault handler runs at the highest exception priority and no other code can preempt it. The handler never returns; it writes the fault record and resets the processor.

## Feature flags

### SDK features

| Feature | Effect |
|---|---|
| `cortex-m` | Enables the HardFault handler, pulls in `cortex-m` and `cortex-m-rt` |
| `defmt` | Enables the `defmt_sink` module that captures defmt output into the TraceBuffer |
| `embassy` | Enables `AsyncChunkTransport` trait and async upload in `UploadManager` |
| `postcard` | Enables optional postcard-based serialization (alternative to hand-rolled encoding) |
| `usb-cdc` | USB CDC transport via `embassy-usb` |
| `http` | WiFi/HTTP transport via `reqwless` + `embedded-io-async` |
| `lora` | LoRa transport via `embedded-hal` SPI |

### Gateway features

| Feature | Default | Effect |
|---|---|---|
| `usb` | Yes | USB serial support via `serialport` |
| `ble` | Yes | BLE scanning via `btleplug` |

For host testing, disable all SDK features:

```bash
cargo test -p ferrite-sdk --no-default-features
```

## Server architecture

The `ferrite-server` binary is a standard Rust CLI application built with:

- **clap** for argument parsing (subcommands: `serve`, `report`, `faults`, `metrics`)
- **axum** for the HTTP server with middleware (auth, rate limiting, CORS)
- **rusqlite** for SQLite storage (WAL mode, foreign keys enabled)
- **tokio** for the async runtime and background tasks
- **dotenvy** for `.env` file configuration
- **jsonwebtoken** for Keycloak JWT validation with JWKS caching

The server re-implements chunk decoding in `ingest.rs` (it does not depend on the `ferrite-sdk` crate, since it needs `std`). It parses each chunk type's payload and stores the data in normalized SQLite tables.

### HTTP API endpoints

| Method | Path | Description |
|---|---|---|
| POST | `/ingest/chunks` | Accept binary chunk data |
| POST | `/ingest/elf` | Upload an ELF file for symbolication |
| GET | `/devices` | List all known devices |
| POST | `/devices/register` | Register a device |
| GET | `/devices/{id}/faults` | List fault events for a device |
| GET | `/devices/{id}/metrics` | List metric entries for a device |
| GET | `/groups` | List device groups |
| GET | `/ota/targets` | List OTA firmware targets |
| GET | `/events/stream` | SSE live event stream |
| GET | `/metrics/prometheus` | Prometheus metrics |
| GET | `/admin/backup` | Download database backup |

See [Configuration](../server/configuration) for the complete API reference.

### Middleware stack

1. **CORS** — permissive cross-origin headers (configurable via `CORS_ORIGIN`)
2. **Authentication** — path-based routing: public endpoints, API-key-protected ingest, user-auth-protected API
3. **Rate limiting** — optional per-IP token bucket on `/ingest` and `/auth` paths

### Background tasks

- **Retention cleanup** — runs every hour, deletes data older than `RETENTION_DAYS`
- **Offline alerting** — runs every 60 seconds, checks for devices not seen recently
- **Rate limit cleanup** — runs every 60 seconds, removes stale IP entries

### Symbolication

When a `FaultRecord` chunk arrives, the server attempts to resolve the PC address using `arm-none-eabi-addr2line` (10-second timeout). ELF files are uploaded separately via `POST /ingest/elf` and stored in the configured `--elf-dir`. The resolved symbol (e.g., `main at src/main.rs:42`) is stored alongside the fault record in SQLite.
