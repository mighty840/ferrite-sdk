# Introduction

ferrite-sdk is an embedded Rust firmware observability SDK designed for ARM Cortex-M microcontrollers. It provides crash capture, metrics collection, structured logging, and reboot reason tracking -- all in a `no_std`, zero-allocation library that uploads telemetry over any transport you choose.

## What problem does it solve?

Embedded devices deployed in the field are difficult to debug. When a sensor node crashes at 2 AM, you typically have no stack trace, no logs, and no idea what happened. ferrite-sdk solves this by:

1. **Capturing HardFaults automatically.** The SDK installs a Cortex-M exception handler that saves all registers (R0-R12, SP, LR, PC, xPSR), the CFSR/HFSR/MMFAR/BFAR fault status registers, and a 64-byte stack snapshot into retained RAM before resetting.

2. **Recording why the device rebooted.** After every reset, your firmware reads the MCU's reset-cause register and stores the result. On the next upload cycle, the server receives a structured reboot event.

3. **Collecting metrics without heap allocation.** Counters, gauges, and histograms are stored in a fixed-capacity ring buffer (`MetricsBuffer<N>`). When the buffer is full, the oldest metric entry is evicted. Metric keys are limited to 32 bytes to keep things compact.

4. **Buffering structured trace logs.** If you use `defmt`, the SDK captures formatted log output into a circular `TraceBuffer<N>`. Frames are framed with a level byte, a 4-byte tick count, a length-prefixed payload, and a sentinel byte.

5. **Uploading everything as binary chunks.** All telemetry is encoded into a compact binary wire format (max 256 bytes per chunk) with CRC-16 integrity checks. You implement one trait -- `ChunkTransport` in Rust or a `send_chunk` callback in C -- and the SDK handles the rest.

## Who is it for?

- **Embedded Rust developers** shipping firmware on Cortex-M3, M4, or M4F targets (thumbv7m, thumbv7em-none-eabi, thumbv7em-none-eabihf).
- **C/C++ firmware teams** who want the same observability without rewriting their RTOS. The `ferrite-ffi` crate produces a static library (`.a`) and a C header that work with Zephyr, FreeRTOS, or any bare-metal C project.
- **Platform engineers** building fleet-wide device monitoring. The companion `ferrite-server` ingests chunks over HTTP, stores them in SQLite, and symbolicates fault addresses using `arm-none-eabi-addr2line`.

## Repository structure

| Crate | Description |
|---|---|
| `ferrite-sdk` | Core `no_std` SDK -- metrics, faults, trace, chunks, transport |
| `ferrite-embassy` | Embassy async task for periodic/triggered uploads |
| `ferrite-rtic` | RTIC resource wrapper and blocking upload helper |
| `ferrite-ffi` | C FFI static library (`libferrite_ffi.a`) |
| `ferrite-server` | Companion CLI and HTTP ingestion server |

## Design principles

- **No alloc, anywhere.** The SDK never calls `alloc`. All buffers are fixed-size and stack- or static-allocated.
- **No panics in production code paths.** Functions return `Result<T, SdkError>` or silently drop data when buffers overflow. The only panic is calling `init()` twice (which is a programming error).
- **Feature flags gate all hardware dependencies.** The `cortex-m` feature pulls in `cortex-m` and `cortex-m-rt`; the `defmt` feature pulls in the defmt logger; the `embassy` feature enables async transport. With all features disabled, the SDK compiles and tests on the host.
- **Critical sections via `critical-section` crate.** This lets the same code run on real hardware (using `cortex-m` critical sections) and in host tests (using a `std` mutex implementation).
- **Hand-rolled binary encoding.** The chunk wire format is simple enough to implement without serde or postcard, though a `postcard` feature is available for those who prefer it.

## Memory overhead

With default buffer sizes (32 metric entries, 512-byte trace buffer):

| Resource | Usage |
|---|---|
| RAM (retained) | 256 bytes (`.uninit.ferrite` section) |
| RAM (runtime) | ~1.4 KB (MetricsBuffer + TraceBuffer + SdkState) |
| Flash | ~6 KB (depends on enabled features) |

These numbers scale with the const-generic buffer size parameters. A `MetricsBuffer<8>` with a `TraceBuffer<128>` can bring total RAM under 600 bytes.

## Next steps

- [Quickstart](./quickstart) -- get a working example in 10 minutes
- [Core Concepts](./concepts) -- understand retained RAM, chunks, and transport
- [Architecture](./architecture) -- module-level data flow diagram
