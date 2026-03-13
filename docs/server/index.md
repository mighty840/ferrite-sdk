# Server Overview

`ferrite-server` is the companion ingestion server for ferrite-sdk. It receives binary chunk data over HTTP, decodes it, stores it in a SQLite database, and symbolicates fault addresses using ELF debug info.

## Features

- **Chunk ingestion** — accepts raw binary data (concatenated chunks) via `POST /ingest/chunks`
- **ELF upload** — stores firmware ELF files for fault symbolication via `POST /ingest/elf`
- **Device tracking** — auto-registers devices on first contact, tracks firmware version and status
- **Device registration** — manual and bulk registration with names, tags, and device keys
- **Fault storage** — complete crash dumps with registers, stack snapshots, and resolved symbols
- **Metrics storage** — counter, gauge, and histogram metrics with timestamps
- **Reboot tracking** — reboot events with reason codes and boot sequence numbers
- **Dual-mode authentication** — Keycloak OIDC or HTTP Basic auth, auto-detected
- **Role-based access control** — Viewer, Provisioner, and Admin roles
- **Device groups** — organize devices into fleets by location, project, or tag
- **OTA firmware updates** — set target firmware versions per device with download URLs
- **Server-Sent Events** — real-time event stream for dashboards and integrations
- **Prometheus metrics** — `/metrics/prometheus` endpoint for monitoring
- **Alerting** — webhook notifications on faults and device offline events
- **Data retention** — automatic purge of old metrics, faults, and reboots
- **Rate limiting** — per-IP token bucket on ingest and auth endpoints
- **Database backup** — download consistent SQLite snapshots via API
- **Chunk encryption** — AES-128-CCM decryption for encrypted chunks
- **CLI reports** — print device summaries, recent faults, and metrics from the command line

## Architecture

```
                                    ┌─────────────────────────────────────┐
                                    │         ferrite-server              │
Devices ──[chunks over HTTP]──────> │                                     │
                                    │  Chunk decoder                      │
Gateway ──[chunks over HTTP]──────> │    ├── Fault handler → addr2line    │
                                    │    ├── Metrics handler              │
Dashboard ──[REST API / SSE]──────> │    ├── Heartbeat handler            │
                                    │    ├── OTA request handler          │
Prometheus ──[/metrics/prometheus]─> │    └── Encrypted chunk decryptor   │
                                    │                                     │
                                    │  SQLite (WAL mode)                  │
                                    │    ├── devices, faults, metrics     │
                                    │    ├── reboots, groups, ota_targets │
                                    │    └── group_memberships            │
                                    │                                     │
                                    │  Background tasks                   │
                                    │    ├── Retention cleanup (hourly)   │
                                    │    ├── Offline alerting (60s)       │
                                    │    └── Rate limit cleanup (60s)     │
                                    └─────────────────────────────────────┘
```

## Quick start

```bash
# Start with defaults (Basic auth, admin/admin)
cargo run -p ferrite-server

# Or with environment configuration
cp .env.example .env
# Edit .env with your settings
cargo run -p ferrite-server
```

See [Installation](./installation) and [Configuration](./configuration) for details.
