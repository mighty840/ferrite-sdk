# Edge Gateway

`ferrite-gateway` is a Tokio-based daemon that bridges BLE, USB, Ethernet, and LoRa ferrite devices to a `ferrite-server` instance over HTTP. It runs on a Raspberry Pi, Intel NUC, or any Linux host with the appropriate radio hardware.

## Architecture

```
BLE devices ──────────┐
                      │
USB CDC devices ──────┤
                      ├──> ferrite-gateway ──[HTTP]──> ferrite-server
USB VCP (UART) ───────┤         │
                      │    SQLite buffer
Ethernet devices ─────┘   (offline queue)
 (HTTP POST :4001)
```

The gateway receives raw binary chunks from connected devices via 4 transport channels, validates CRC integrity, batches chunks from the same upload session (200ms window), and forwards them to the server. If the server is unreachable, chunks are buffered in a local SQLite database and retried with exponential backoff.

## Features

- **BLE scanning** — discovers ferrite devices by GATT service UUID or device name, subscribes to chunk notifications
- **USB CDC/VCP serial** — reads chunks from multiple USB-connected devices at configurable baud rate, sets DTR for CDC devices
- **HTTP ingest** — accepts chunk POSTs from Ethernet/WiFi devices on a configurable port (industrial edge pattern)
- **Chunk batching** — groups chunks from the same upload session (200ms window) into a single server POST, preserving DeviceInfo context
- **Offline buffering** — SQLite queue persists chunks across gateway restarts
- **Automatic retry** — exponential backoff with health check polling
- **API key auth** — includes `X-API-Key` header if configured

## Installation

```bash
# Build with all transports
cargo build -p ferrite-gateway --release

# Build with only USB support
cargo build -p ferrite-gateway --release --no-default-features --features usb

# Build with only BLE support
cargo build -p ferrite-gateway --release --no-default-features --features ble
```

## Usage

```bash
# Basic usage — USB device forwarding to local server
ferrite-gateway --server http://localhost:4000 --usb-port /dev/ttyACM0

# BLE scanning + USB
ferrite-gateway --server http://ferrite.local:4000 --ble --usb-port /dev/ttyACM0

# Custom baud rate and buffer location
ferrite-gateway \
  --server http://ferrite.local:4000 \
  --usb-port /dev/ttyACM0 \
  --usb-baud 230400 \
  --buffer-db /var/lib/ferrite/gateway.db
```

## CLI options

| Flag | Default | Description |
|---|---|---|
| `--server` | `FERRITE_SERVER_URL` env | Server URL |
| `--usb-port` | `USB_PORTS` env | USB serial port path (repeat for several) |
| `--usb-baud` | `USB_BAUD` env, else `115200` | USB baud rate |
| `--ble` | false | Enable BLE scanning |
| `--http-ingest-port` | `4001` | HTTP ingest listener port (0 = disabled) |
| `--buffer-db` | `GATEWAY_BUFFER_DB` env, else `ferrite-gateway.db` | SQLite buffer path |

A flag given on the command line wins over its environment variable.

## Environment variables

The gateway also reads a `.env` file in its working directory.

| Variable | Description |
|---|---|
| `FERRITE_SERVER_URL` | Server URL (overridden by `--server`) |
| `INGEST_API_KEY` | Server's ingest API key, sent as `X-API-Key` |
| `BASIC_AUTH_USER`, `BASIC_AUTH_PASS` | Login for the OTA relay |
| `USB_PORTS` | Comma-separated USB serial port paths (or a single `USB_PORT`) |
| `USB_BAUD` | USB baud rate (overridden by `--usb-baud`) |
| `GATEWAY_BUFFER_DB` | SQLite buffer path (overridden by `--buffer-db`) |
| `GATEWAY_MAX_RETRIES` | HTTP forwarding attempts per request (default 3) |

## Chunk framing

The gateway uses a streaming `ChunkFramer` that handles byte-level synchronization:

1. Scans for magic byte `0xEC`
2. Reads 8-byte header to get payload length
3. Reads payload + 2-byte CRC
4. Validates CRC-16/CCITT-FALSE
5. Forwards valid chunks, discards corrupted data

This handles split reads across USB/BLE packet boundaries and recovers from partial transmissions.

## Running as a service

```ini
# /etc/systemd/system/ferrite-gateway.service
[Unit]
Description=Ferrite Edge Gateway
After=network.target bluetooth.target

[Service]
ExecStart=/usr/local/bin/ferrite-gateway \
  --server http://ferrite.local:4000 \
  --ble \
  --buffer-db /var/lib/ferrite/gateway.db
Restart=always
Environment=INGEST_API_KEY=your-api-key
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```
