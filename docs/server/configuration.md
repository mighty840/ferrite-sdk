# Server Configuration

The server is configured entirely through command-line arguments. There is no configuration file.

## Command-line arguments

```
iotai-server [OPTIONS] [COMMAND]
```

### Global options

| Flag | Default | Description |
|---|---|---|
| `--http <ADDR>` | `0.0.0.0:4000` | HTTP listen address (ip:port) |
| `--db <PATH>` | `./iotai.db` | SQLite database file path |
| `--elf-dir <PATH>` | `./elfs` | Directory for uploaded ELF files |
| `--addr2line <PATH>` | Auto-detect | Path to `arm-none-eabi-addr2line` binary |

### Subcommands

| Command | Description |
|---|---|
| `serve` | Start the HTTP server (default if no command given) |
| `report` | Print a summary of all devices to stdout |
| `faults` | List recent fault events to stdout |
| `metrics` | List recent metrics to stdout |

### Examples

```bash
# Start with all defaults
iotai-server

# Custom port and database location
iotai-server --http 127.0.0.1:8080 --db /var/lib/iotai/data.db

# Print a device report without starting the server
iotai-server --db /var/lib/iotai/data.db report

# List recent faults
iotai-server --db /var/lib/iotai/data.db faults
```

## HTTP API endpoints

### `POST /ingest/chunks`

Accepts a raw binary body containing one or more concatenated wire-format chunks. The server decodes each chunk, processes the payload, and stores results in SQLite.

**Headers:**
- `X-Device-Id` (optional): Fallback device ID if no DeviceInfo chunk is present in the payload.

**Response:**

```json
{
  "ok": true,
  "chunks_received": 4,
  "errors": []
}
```

Status codes: `200 OK` if all chunks processed, `207 Multi-Status` if some chunks had errors.

### `POST /ingest/elf`

Accepts a raw binary ELF file. The file is stored in `--elf-dir` for later symbolication.

**Headers:**
- `X-Firmware-Version` (recommended): The firmware version string. The ELF file is saved as `{version}.elf`.

**Example:**

```bash
curl -X POST http://localhost:4000/ingest/elf \
  -H "X-Firmware-Version: 1.2.3" \
  --data-binary @target/thumbv7em-none-eabihf/release/my-firmware
```

### `GET /devices`

List all known devices.

```json
{
  "devices": [
    {
      "id": 1,
      "device_id": "sensor-42",
      "firmware_version": "1.2.3",
      "build_id": 0,
      "first_seen": "2025-01-15 10:30:00",
      "last_seen": "2025-01-15 11:45:00"
    }
  ]
}
```

### `GET /devices/{id}/faults`

List fault events for a device (up to 100, newest first).

### `GET /devices/{id}/metrics`

List metric entries for a device (up to 200, newest first).

## SQLite schema

The server creates four tables:

- `devices` -- device_id (unique), firmware_version, build_id, first_seen, last_seen
- `fault_events` -- fault_type, pc, lr, cfsr, hfsr, mmfar, bfar, sp, stack_snapshot (JSON), symbol
- `metrics` -- key, metric_type, value_json, timestamp_ticks
- `reboot_events` -- reason, extra, boot_sequence, uptime_before_reboot

All tables use SQLite WAL mode for concurrent read/write performance. Foreign keys are enabled.

## CORS

The server enables permissive CORS headers (`Access-Control-Allow-Origin: *`) to allow the dashboard frontend to connect from any origin.
