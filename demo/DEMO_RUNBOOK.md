# Ferrite Fleet Demo — Runbook

Live demo of 4+ embedded devices reporting telemetry through mixed transports
to a Raspberry Pi edge gateway running ferrite-server + dashboard.

## Fleet Topology

```
┌──────────────────────────────────────────────────────────────────┐
│  Raspberry Pi (Gateway + Server)                                 │
│  ┌─────────────────┐  ┌──────────────────┐  ┌───────────────┐   │
│  │ ferrite-server   │  │ ferrite-gateway   │  │ Dashboard     │   │
│  │ :4000            │◄─│ USB + BLE bridge  │  │ :4000/        │   │
│  │ SQLite + REST    │  │ offline buffer    │  │ (Dioxus WASM) │   │
│  └────────▲─────────┘  └──▲────────▲──────┘  └───────────────┘   │
│           │               │        │                              │
└───────────┼───────────────┼────────┼──────────────────────────────┘
            │               │        │
     ┌──────┴──────┐  ┌────┴────┐  ┌┴──────────┐  ┌──────────────┐
     │ ESP32-C3    │  │Nucleo   │  │ nRF5340-DK│  │NUCLEO-WL55JC1│
     │ WiFi/HTTP   │  │L4A6ZG   │  │ BLE       │  │LoRa SubGHz   │
     │ Direct POST │  │USB CDC  │  │ GATT notif│  │915 MHz P2P   │
     └─────────────┘  └─────────┘  └───────────┘  └──────────────┘
```

## Pre-Demo Checklist

### Hardware
- [ ] RPi powered on, Ethernet/WiFi connected to same network
- [ ] Nucleo-L4A6ZG connected to RPi via USB (shows as `/dev/ttyACM0`)
- [ ] nRF5340-DK powered on (BLE advertising)
- [ ] ESP32-C3 powered on (WiFi SSID configured, same network as RPi)
- [ ] NUCLEO-WL55JC1 powered on (LoRa TX active)
- [ ] All boards flashed with latest firmware (see Flash section)

### Software
- [ ] ferrite-server running: `sudo systemctl status ferrite-server`
- [ ] ferrite-gateway running: `sudo systemctl status ferrite-gateway`
- [ ] Dashboard accessible: `http://<rpi-ip>:4000`
- [ ] Logged into dashboard (admin/changeme or configured credentials)

## Flash Firmware

From the development machine with probe-rs and espflash installed:

```bash
# ESP32-C3 (WiFi/HTTP) — edit WIFI_SSID/WIFI_PASS in src/main.rs first
cd examples/embassy-esp32c3
# Update SERVER_URL to point to RPi IP
cargo run --release

# nRF5340-DK (BLE)
cd examples/embassy-nrf5340
cargo run --release

# NUCLEO-WL55JC1 (LoRa)
cd examples/embassy-stm32wl55
cargo run --release

# Nucleo-L4A6ZG (USB) — uses existing nRF52840 example adapted for L4A6ZG
cd examples/embassy-nrf52840
cargo run --release
```

## Demo Scenarios

### Scenario 1: Live Telemetry (2 min)

**Goal**: Show real-time data flowing from all 4 devices.

1. Open dashboard at `http://<rpi-ip>:4000`
2. Navigate to **Devices** page — all 4 devices should appear:
   - `esp32c3-fleet-01` (WiFi/HTTP)
   - `nrf5340-fleet-01` (BLE)
   - `stm32wl55-fleet-01` (LoRa)
   - `nrf52840-example-01` (USB)
3. Click any device → **Device Detail** page
4. Show **Metrics** tab — `loop_count` incrementing, `uptime_seconds` growing
5. Show **Fleet Overview** page — all devices on one screen with status indicators

**Talking points**:
- Mixed transports, single dashboard
- Gateway bridges BLE/USB/LoRa to HTTP automatically
- ESP32-C3 bypasses gateway entirely (direct WiFi POST)
- All data stored in SQLite, no cloud dependency

### Scenario 2: Fault Recovery (3 min)

**Goal**: Demonstrate crash detection and symbolicated stack traces.

1. Trigger a fault on the nRF5340 (press reset button while holding a GPIO, or use a debug probe to inject a HardFault)
2. Wait for the device to reboot and re-register
3. Navigate to **Faults** page in dashboard
4. Show the fault record: PC, LR, stack pointer, reboot reason
5. Upload the ELF file via the dashboard **Settings** page
6. Show symbolicated fault — source file + line number

**Talking points**:
- Retained RAM survives reboot (magic number validated)
- Fault records include full exception frame
- ELF symbolication via addr2line (no debug build on device)

### Scenario 3: Offline Resilience (2 min)

**Goal**: Show gateway buffering when server is temporarily down.

1. Stop ferrite-server: `sudo systemctl stop ferrite-server`
2. Wait 30 seconds — devices keep sending, gateway buffers to SQLite
3. Show gateway logs: `journalctl -u ferrite-gateway -f`
   - Should see retry attempts with exponential backoff
4. Restart server: `sudo systemctl start ferrite-server`
5. Watch dashboard — buffered data appears within seconds
6. Show no data loss by checking metric continuity (no gaps in `loop_count`)

**Talking points**:
- Gateway has SQLite offline buffer
- Exponential backoff prevents server overload on recovery
- Zero data loss during outage window

### Scenario 4: Device Comparison (1 min)

**Goal**: Compare metrics across fleet devices.

1. Navigate to **Compare** view in dashboard
2. Select `esp32c3-fleet-01` and `nrf5340-fleet-01`
3. Show side-by-side `uptime_seconds` metrics
4. Point out different reporting intervals (WiFi = 5s, BLE = 30s)

### Scenario 5: Unplug/Replug USB (1 min)

**Goal**: Show transport disconnect/reconnect handling.

1. Physically unplug Nucleo-L4A6ZG USB cable
2. Show gateway log detecting disconnect
3. Dashboard shows device going stale (no recent data)
4. Replug USB cable
5. Gateway auto-reconnects, data flows resume

## Verification Commands

Run these on the RPi to verify the stack is healthy:

```bash
# Check services
sudo systemctl status ferrite-server ferrite-gateway

# Server health
curl -s http://localhost:4000/health | jq .

# List registered devices
curl -s -u admin:changeme http://localhost:4000/devices | jq .

# Recent faults
curl -s -u admin:changeme http://localhost:4000/devices/esp32c3-fleet-01/faults | jq .

# Recent metrics
curl -s -u admin:changeme http://localhost:4000/devices/nrf5340-fleet-01/metrics?limit=10 | jq .

# Gateway logs (live)
journalctl -u ferrite-gateway -f

# USB device check
ls -la /dev/ttyACM*

# BLE scan (verify nRF5340 advertising)
sudo bluetoothctl scan on
# Look for device advertising FE771E00-0001-... service UUID

# Server Prometheus metrics
curl -s http://localhost:4000/metrics
```

## Troubleshooting

| Symptom | Check | Fix |
|---------|-------|-----|
| No devices in dashboard | `curl localhost:4000/health` | Restart ferrite-server |
| ESP32-C3 not connecting | Serial monitor shows WiFi status | Check SSID/password in firmware |
| BLE device not found | `sudo bluetoothctl scan on` | Ensure nRF5340 is advertising, gateway has `--ble` flag |
| USB device not seen | `ls /dev/ttyACM*` | Check USB cable, `ferrite` user in `dialout` group |
| LoRa no data | Gateway LoRa logs | Verify frequency match (915 MHz), antenna connected |
| Dashboard blank | Browser console (F12) | Check CORS, API proxy config |
| Auth failures | Server logs | Verify matching credentials in server.env and gateway.env |
