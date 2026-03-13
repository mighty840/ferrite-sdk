# Edge Gateway

`ferrite-gateway` is a Tokio-based daemon that bridges BLE, USB, and LoRa ferrite devices to a `ferrite-server` instance over HTTP. It runs on a Raspberry Pi, Intel NUC, or any Linux host with the appropriate radio hardware.

## Architecture

```
BLE devices ──┐
              ├──> ferrite-gateway ──[HTTP]──> ferrite-server
USB devices ──┘         |
                   SQLite buffer
                  (offline queue)
```

The gateway receives raw binary chunks from connected devices, validates CRC integrity, and forwards them to the server. If the server is unreachable, chunks are buffered in a local SQLite database and retried with exponential backoff.

## Features

- **BLE scanning** — discovers ferrite devices by GATT service UUID, subscribes to chunk notifications
- **USB CDC serial** — reads chunks from USB-connected devices at configurable baud rate
- **Offline buffering** — SQLite queue persists chunks across gateway restarts
- **Automatic retry** — exponential backoff with health check polling
- **Batch forwarding** — sends buffered chunks in batches for efficiency
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
| `--usb-port` | none | USB serial port path |
| `--usb-baud` | `115200` | USB baud rate |
| `--ble` | false | Enable BLE scanning |
| `--buffer-db` | `ferrite-gateway.db` | SQLite buffer path |

## Environment variables

| Variable | Description |
|---|---|
| `FERRITE_SERVER_URL` | Server URL (overridden by `--server`) |
| `FERRITE_API_KEY` | API key for `/ingest/chunks` |
| `FERRITE_USB_PORT` | USB serial port path |
| `FERRITE_USB_BAUD` | USB baud rate |

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
Environment=FERRITE_API_KEY=your-api-key
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```
