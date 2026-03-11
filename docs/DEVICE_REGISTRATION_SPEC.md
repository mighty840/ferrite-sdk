# Device Discovery & Registration — Feature Specification

## Overview

This feature adds a device provisioning and registration workflow to the Ferrite ecosystem. During end-of-line (EOL) manufacturing, each device is assigned a unique **device key** that identifies it throughout its lifecycle. The key is generated collaboratively between the EOL provisioning tool and the firmware, stored in retained RAM, and included in every heartbeat message for packet identification.

## Device Key Format

A 32-bit unsigned integer, displayed as 8 hex characters (e.g., `A3-00F1B2`).

```
Bits [31:24]  — Owner Prefix (8 bits)
Bits [23:0]   — Device Suffix (24 bits)
```

### Owner Prefix (8 bits)
Derived from the first byte of `SHA-256(user_subject_id)` where `user_subject_id` is the Keycloak `sub` claim or the basic auth username. This namespaces keys by the provisioning operator, reducing collision probability across independent provisioning stations.

- In Keycloak mode: `SHA-256(sub)[0]`
- In Basic auth mode: `SHA-256(username)[0]`
- If no user context is available (standalone mode): `0x00`

### Device Suffix (24 bits)
Generated on the device using available entropy:
- **With hardware RNG** (preferred): 24 random bits from the MCU's RNG peripheral
- **Without hardware RNG**: XOR of unique chip ID bytes, SysTick, and a monotonic counter from retained RAM

This gives ~16.7 million unique devices per owner prefix. With 256 possible prefixes, the total key space is ~4.3 billion.

### Idempotency
Once a key is written to retained RAM, calling the provisioning function again returns the existing key rather than generating a new one. To force re-provisioning, the retained RAM must be explicitly cleared (via a dedicated command or full chip erase).

## Architecture

### Component Interactions

```
+-------------------+     UART      +------------------+     HTTP      +------------------+
| Device Firmware   | <-----------> | EOL Provisioning | -----------> | Ferrite Server   |
| (ferrite-sdk)     |   provision   | Tool (ferrite-   |   register   |                  |
|                   |   command     | provision)       |   device     |                  |
+-------------------+               +------------------+              +------------------+
                                          |                                    ^
                                          | CSV/JSON                           |
                                          v                                    |
                                    +------------------+     bulk        +------------------+
                                    | Local Export     | ------------> | Dashboard UI     |
                                    | (file)           |   import      |                  |
                                    +------------------+               +------------------+
```

## 1. ferrite-sdk Changes

### 1a. New field in `RetainedBlock`
```rust
pub struct RetainedBlock {
    pub header: RetainedHeader,
    pub reboot_reason: RebootReasonRecord,
    pub fault_record: FaultRecord,
    pub device_key: DeviceKeyRecord,   // NEW: 8 bytes
    pub metrics_dirty: bool,
    pub _pad: [u8; 3],
}
```

### 1b. `DeviceKeyRecord` (in `device_key.rs`)
```rust
const DEVICE_KEY_MAGIC: u32 = 0xFE_D1_5C_07;  // "FErriteDevice07"

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DeviceKeyRecord {
    pub magic: u32,         // DEVICE_KEY_MAGIC when valid
    pub key: u32,           // The 32-bit device key
}
```

### 1c. Public API
```rust
/// Provision a device key. If a key already exists in retained RAM, returns it.
/// Otherwise, generates a new key using the provided owner_prefix and entropy.
///
/// - `owner_prefix`: 8-bit namespace from SHA-256 of the provisioning user's ID
/// - `entropy_seed`: caller-provided entropy (e.g., from hardware RNG or SysTick)
///
/// Returns the 32-bit device key.
pub fn provision_device_key(owner_prefix: u8, entropy_seed: u32) -> u32;

/// Read the device key from retained RAM. Returns None if not provisioned.
pub fn device_key() -> Option<u32>;

/// Clear the device key from retained RAM. Used for re-provisioning.
pub fn clear_device_key();
```

### 1d. Heartbeat Changes
The heartbeat payload grows from 20 to 24 bytes:
```
[uptime_ticks: 8][free_stack: 4][metrics_count: 4][frames_lost: 4][device_key: 4]
```

The device_key field is `0x00000000` if not provisioned (server treats as "unregistered").

### 1e. UART Provisioning Protocol
Simple request/response over UART for the EOL tool to communicate with firmware:

| Command | Byte | Direction | Payload | Response |
|---------|------|-----------|---------|----------|
| PING | `0x50` | Tool -> Device | none | `0x50 0x4F 0x4B` ("POK") |
| PROVISION | `0x52` | Tool -> Device | `[owner_prefix:1][entropy:4]` | `[0x52][key:4]` (5 bytes) |
| READ_KEY | `0x4B` | Tool -> Device | none | `[0x4B][key:4]` or `[0x4B][0x00:4]` |
| CLEAR_KEY | `0x43` | Tool -> Device | none | `[0x43][0x01]` (ACK) |
| READ_INFO | `0x49` | Tool -> Device | none | `[0x49][fw_ver_len:1][fw_ver:N][build_id:8]` |

The firmware implements a provisioning command handler that listens on UART during a configurable window at startup (e.g., first 5 seconds, or until `init()` is called).

## 2. ferrite-provision Tool

A standalone Rust CLI binary (`ferrite-provision/`) that runs on the provisioning workstation.

### Usage
```bash
# Interactive: provision a single device
ferrite-provision --port /dev/ttyACM0 --baud 115200

# Batch: provision and register with server
ferrite-provision --port /dev/ttyACM0 --server http://ferrite.local:4000 \
    --user admin --password admin

# Export to file (offline mode)
ferrite-provision --port /dev/ttyACM0 --output devices.csv

# Bulk import from file to server
ferrite-provision import --file devices.csv --server http://ferrite.local:4000 \
    --user admin --password admin
```

### Output Format (CSV)
```csv
device_key,firmware_version,build_id,provisioned_at,provisioned_by
A300F1B2,1.4.2,0xDEADBEEF,2026-03-11T14:30:00Z,admin
```

### Output Format (JSON)
```json
[
  {
    "device_key": "A300F1B2",
    "firmware_version": "1.4.2",
    "build_id": "0xDEADBEEF",
    "provisioned_at": "2026-03-11T14:30:00Z",
    "provisioned_by": "admin"
  }
]
```

## 3. Server Changes

### 3a. Database Schema
Add columns to `devices` table:
```sql
ALTER TABLE devices ADD COLUMN device_key INTEGER UNIQUE;
ALTER TABLE devices ADD COLUMN name TEXT NOT NULL DEFAULT '';
ALTER TABLE devices ADD COLUMN status TEXT NOT NULL DEFAULT 'registered';
ALTER TABLE devices ADD COLUMN tags TEXT NOT NULL DEFAULT '[]';
ALTER TABLE devices ADD COLUMN provisioned_by TEXT;
ALTER TABLE devices ADD COLUMN provisioned_at TEXT;
```

`status` values: `registered` (provisioned but never connected), `online` (heartbeat within last 60s), `offline` (heartbeat older than 60s), `degraded` (has unresolved faults).

### 3b. New API Endpoints

```
POST   /devices/register          — Register one device
POST   /devices/register/bulk     — Bulk register from CSV/JSON
GET    /devices                   — List devices (updated to include new fields)
GET    /devices/:key              — Get device by key (hex string)
PUT    /devices/:key              — Update device name/tags
DELETE /devices/:key              — Unregister device
```

#### POST /devices/register
```json
{
  "device_key": "A300F1B2",
  "name": "Temperature Sensor A",
  "tags": ["production", "floor-1"],
  "firmware_version": "1.4.2",
  "build_id": "0xDEADBEEF"
}
```
Response: `201 Created` with the device record.

#### POST /devices/register/bulk
Accepts `Content-Type: application/json` (array of registration objects) or `Content-Type: text/csv`.

### 3c. Heartbeat Device Resolution
When a heartbeat arrives with a non-zero `device_key`, the server:
1. Looks up the device by `device_key` (not `device_id` string)
2. Updates `last_seen` and computes status
3. If no device record exists for that key, creates a "discovered" entry (auto-discovered but unregistered)

## 4. Dashboard Changes

### 4a. Remove Mock Data
Replace all hardcoded data in pages with API calls using the existing `ApiClient`.

### 4b. Device Registration Page
New page at route `/devices/register` with:
- **Single registration form**: device key input (hex), name, tags
- **Bulk import**: file upload (CSV/JSON) with preview table
- Validation: hex format, key length, duplicate detection

### 4c. Devices Page Updates
- Show real data from server
- Status indicators (online/offline/registered/degraded)
- "Register Device" button linking to registration page
- Empty state when no devices exist

### 4d. Other Pages
- Dashboard: real device counts, fault counts from API
- Faults: real fault data from API
- Metrics: real metric data from API

## 5. Security Considerations

- The device key is NOT a secret — it's an identifier, like a MAC address
- Authentication to the server uses the existing auth middleware (Basic/Keycloak)
- The `INGEST_API_KEY` protects the ingest endpoint independently
- The owner_prefix provides provenance (who provisioned it) but is not a security boundary
- Device keys can be rotated by clearing retained RAM and re-provisioning

## 6. Migration Path

Existing devices that send `DeviceInfo` chunks with a string `device_id` but no `device_key` in heartbeats will continue to work. The server falls back to string-based device_id matching when device_key is zero. This provides backward compatibility during rollout.
