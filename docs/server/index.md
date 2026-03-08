# Server Overview

`iotai-server` is the companion ingestion server for iotai-sdk. It receives binary chunk data over HTTP, decodes it, stores it in a SQLite database, and optionally symbolicates fault addresses using ELF debug info.

## Features

- **Chunk ingestion** -- accepts raw binary data (concatenated chunks) via `POST /ingest/chunks`
- **ELF upload** -- stores firmware ELF files for fault symbolication via `POST /ingest/elf`
- **Device tracking** -- automatically registers devices on first contact, tracks firmware version and last-seen time
- **Fault storage** -- stores complete crash dumps with registers, stack snapshots, and resolved symbols
- **Metrics storage** -- stores counter, gauge, and histogram metrics with timestamps
- **Reboot tracking** -- stores reboot events with reason codes and boot sequence numbers
- **CLI reports** -- print device summaries, recent faults, and metrics from the command line
- **REST API** -- query devices, faults, and metrics over HTTP (JSON responses)

## Architecture

```
Devices ──[binary chunks over HTTP]──> iotai-server
                                          |
                                          v
                                    Chunk decoder
                                          |
                         +----------------+----------------+
                         |                |                |
                    Fault handler    Metrics handler   Heartbeat handler
                         |                |                |
                         v                v                v
                    +----+----+     +-----+----+     Log + touch
                    | addr2line|    | SQLite   |     device
                    | (ELF)   |    | insert   |
                    +----+----+    +----------+
                         |
                         v
                    SQLite insert
                    (with symbol)
```

## Quick start

```bash
cd iotai-server
cargo run -- --http 0.0.0.0:4000 --db ./iotai.db --elf-dir ./elfs
```

See [Installation](./installation) and [Configuration](./configuration) for details.
