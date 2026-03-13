# Server Configuration

The server is configured through a combination of command-line arguments and environment variables. Environment variables are loaded from a `.env` file via `dotenvy` (see `.env.example`).

## Command-line arguments

```
ferrite-server [OPTIONS] [COMMAND]
```

### Global options

| Flag | Default | Description |
|---|---|---|
| `--http <ADDR>` | `0.0.0.0:4000` | HTTP listen address (ip:port) |
| `--db <PATH>` | `./ferrite.db` | SQLite database file path |
| `--elf-dir <PATH>` | `./elfs` | Directory for uploaded ELF files |
| `--addr2line <PATH>` | Auto-detect | Path to `arm-none-eabi-addr2line` binary |

### Subcommands

| Command | Description |
|---|---|
| `serve` | Start the HTTP server (default if no command given) |
| `report` | Print a summary of all devices to stdout |
| `faults` | List recent fault events to stdout |
| `metrics` | List recent metrics to stdout |

## Environment variables

### Authentication

| Variable | Default | Description |
|---|---|---|
| `KEYCLOAK_URL` | — | Keycloak base URL (enables OIDC mode) |
| `KEYCLOAK_REALM` | — | Keycloak realm name |
| `KEYCLOAK_CLIENT_ID` | — | Dashboard SPA client ID |
| `KEYCLOAK_CLIENT_SECRET` | — | (Optional) Confidential client secret |
| `BASIC_AUTH_USER` | `admin` | Primary basic auth username |
| `BASIC_AUTH_PASS` | `admin` | Primary basic auth password |
| `BASIC_AUTH_USERS` | — | Additional users (format below) |

**`BASIC_AUTH_USERS` format:** `user1:pass1:role,user2:pass2:role`

Roles: `admin`, `provisioner`, `viewer`.

### API security

| Variable | Default | Description |
|---|---|---|
| `INGEST_API_KEY` | — | API key for `/ingest/*` endpoints |
| `CORS_ORIGIN` | `*` (all) | Allowed CORS origin |
| `CHUNK_ENCRYPTION_KEY` | — | 32-char hex AES-128 key for encrypted chunks |
| `RATE_LIMIT_RPS` | disabled | Per-IP rate limit (requests/second) |

### Alerting

| Variable | Default | Description |
|---|---|---|
| `ALERT_WEBHOOK_URL` | — | Webhook URL for fault/offline alerts (Slack/Discord compatible) |
| `ALERT_OFFLINE_MINUTES` | `10` | Minutes before a device is marked offline |

### Data retention

| Variable | Default | Description |
|---|---|---|
| `RETENTION_DAYS` | `90` | Auto-purge data older than N days (0 = disabled) |

## HTTP API endpoints

### Health & discovery

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/health` | Public | Server health check |
| GET | `/auth/mode` | Public | Auth mode discovery |
| GET | `/metrics/prometheus` | Public | Prometheus metrics |
| GET | `/events/stream` | Public | SSE live event stream |

### Data ingest

| Method | Path | Auth | Description |
|---|---|---|---|
| POST | `/ingest/chunks` | API key (if configured) | Accept binary chunks |
| POST | `/ingest/elf` | Required | Upload ELF for symbolication (max 50 MB) |

### Device management

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/devices` | User | List all devices |
| POST | `/devices/register` | Provisioner+ | Register a device |
| POST | `/devices/register/bulk` | Provisioner+ | Bulk register devices |
| GET | `/devices/:key` | User | Get device by key |
| PUT | `/devices/:key` | Provisioner+ | Update device metadata |
| DELETE | `/devices/:key` | Admin | Delete a device |

### Device data

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/devices/:id/faults` | User | Device fault events |
| GET | `/devices/:id/metrics` | User | Device metrics |
| GET | `/faults` | User | All fault events |
| GET | `/metrics` | User | All metrics |

### Groups (fleet management)

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/groups` | User | List groups |
| POST | `/groups` | Admin | Create group |
| GET | `/groups/:id` | User | Get group details |
| PUT | `/groups/:id` | Admin | Update group |
| DELETE | `/groups/:id` | Admin | Delete group |
| GET | `/groups/:id/devices` | User | List group devices |
| POST | `/groups/:id/devices/:device_id` | Admin | Add device to group |
| DELETE | `/groups/:id/devices/:device_id` | Admin | Remove device from group |

### OTA firmware updates

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/ota/targets` | User | List OTA targets |
| POST | `/ota/targets` | Admin | Set OTA target for a device |
| GET | `/ota/targets/:device_id` | User | Get target for device |
| DELETE | `/ota/targets/:device_id` | Admin | Remove OTA target |

### Admin

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/admin/backup` | Admin | Download database backup |
| GET | `/admin/retention` | Admin | View retention policy status |

## Ingest endpoint details

### `POST /ingest/chunks`

Accepts a raw binary body containing one or more concatenated wire-format chunks.

**Headers:**
- `X-Device-Id` (optional): Fallback device ID if no DeviceInfo chunk is present
- `X-API-Key` (required if `INGEST_API_KEY` is set): API key for authentication

**Response:**
```json
{
  "ok": true,
  "chunks_received": 4,
  "errors": []
}
```

### `POST /ingest/elf`

Accepts a raw binary ELF file (max 50 MB). Requires authentication.

**Headers:**
- `X-Firmware-Version` (recommended): Version string for the ELF file
- `Authorization`: Bearer token or Basic auth

```bash
curl -X POST http://localhost:4000/ingest/elf \
  -H "X-Firmware-Version: 1.2.3" \
  -H "Authorization: Basic $(echo -n admin:admin | base64)" \
  --data-binary @target/thumbv7em-none-eabihf/release/my-firmware
```

## Prometheus metrics

The `/metrics/prometheus` endpoint exposes:

- `ferrite_devices_total` — total registered devices
- `ferrite_devices_online` — currently online devices
- `ferrite_faults_total` — total fault events
- `ferrite_metrics_total` — total metric data points
- `ferrite_reboots_total` — total reboot events
- `ferrite_groups_total` — number of device groups
- `ferrite_ingest_requests_total` — ingest request counter
- `ferrite_chunks_processed_total` — chunk processing counter
- `ferrite_auth_failures_total` — authentication failure counter
- `ferrite_sse_connections` — active SSE connections

## Server-Sent Events (SSE)

The `/events/stream` endpoint provides real-time updates:

```bash
curl -N http://localhost:4000/events/stream
```

Event types: `heartbeat`, `fault`, `metric`, `reboot`, `device_registered`, `ota_available`.

## SQLite schema

The server creates these tables:

- `devices` — device_id, name, status, firmware_version, device_key, tags, provisioned_by/at
- `fault_events` — fault_type, pc, lr, cfsr, hfsr, mmfar, bfar, sp, stack_snapshot, symbol
- `metrics` — key, metric_type, value_json, timestamp_ticks
- `reboot_events` — reason, extra, boot_sequence, uptime_before_reboot
- `groups` — name, description
- `group_memberships` — group_id, device_id
- `ota_targets` — device_id, target_version, build_id, firmware_url

All tables use SQLite WAL mode. Foreign keys are enabled.
