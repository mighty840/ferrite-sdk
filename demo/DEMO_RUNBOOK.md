# Ferrite Fleet Demo — Runbook

Live demo of 5 embedded devices across 5 firmware stacks, reporting telemetry
through mixed transports to a Raspberry Pi edge gateway and dashboard.

## Fleet Topology

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Host Machine                                                           │
│  ┌──────────────────┐  ┌────────────────────────────────────┐           │
│  │ ferrite-server    │  │ ferrite-dashboard (dx serve)       │           │
│  │ :4000             │  │ :8080 → proxy to :4000             │           │
│  │ SQLite + REST API │  │ Dioxus WASM                        │           │
│  └────────▲──────────┘  └────────────────────────────────────┘           │
│           │                                                              │
└───────────┼──────────────────────────────────────────────────────────────┘
            │ HTTP
┌───────────┼──────────────────────────────────────────────────────────────┐
│  Raspberry Pi (Edge Gateway)                                             │
│  ┌────────┴───────────────────────────────────────────────────────┐      │
│  │ ferrite-gateway                                                │      │
│  │   --usb-port /dev/ttyACM0  --usb-port /dev/ttyACM1            │      │
│  │   --http-ingest-port 4001  --ble                               │      │
│  │   --server http://host:4000                                    │      │
│  └──▲──────────▲──────────────▲───────────────────▲───────────────┘      │
│     │USB       │USB           │Ethernet           │BLE                   │
└─────┼──────────┼──────────────┼───────────────────┼──────────────────────┘
 ┌────┴────┐ ┌───┴──────┐  ┌───┴────────┐  ┌──────┴───────┐
 │WL55JC1  │ │L4A6ZG    │  │H563ZI      │  │nRF5340-DK    │
 │RTIC 2.x │ │Embassy   │  │Embassy     │  │Zephyr C+FFI  │
 │LPUART   │ │USB CDC   │  │Ethernet    │  │BLE GATT      │
 └─────────┘ └──────────┘  └────────────┘  └──────────────┘

 ┌──────────────────┐
 │ESP32-C3          │──WiFi HTTP POST──────────────────────▶ server:4000
 │Embassy (esp-hal) │  (bypasses gateway)
 └──────────────────┘
```

## Device Summary

| Board | MCU | Stack | Transport | Device ID |
|-------|-----|-------|-----------|-----------|
| ESP32-C3 | RISC-V | Embassy (esp-hal 0.23) | WiFi → HTTP | `esp32c3-embassy-01` |
| STM32L4A6 | Cortex-M4 | Embassy (embassy-stm32) | USB CDC | `stm32l4a6-fleet-01` |
| STM32WL55 | Cortex-M4 | RTIC 2.x (stm32wl PAC) | LPUART VCP | `stm32wl55-rtic-01` |
| STM32H563 | Cortex-M33 | Embassy (embassy-stm32) | Ethernet HTTP | `stm32h563-fleet-01` |
| nRF5340 | Cortex-M33 | Zephyr 4.1 (C + FFI) | BLE GATT | `nrf5340-zephyr-01` |

## Pre-Demo Checklist

### Hardware
- [ ] RPi powered on, WiFi for management SSH + Ethernet for H563 direct link
- [ ] WL55 connected to RPi USB (ST-LINK VCP → `/dev/ttyACMx`)
- [ ] L4A6 connected to RPi USB (USB CDC → `/dev/ttyACMx`) — CN13 USB OTG port, NOT CN1 ST-LINK
- [ ] H563 Ethernet cable → RPi Ethernet port (static IP 192.168.0.200 → RPi 192.168.0.103)
- [ ] nRF5340-DK powered (USB to RPi or wall adapter, BLE advertising)
- [ ] ESP32-C3 powered (USB, WiFi SSID configured, same network as host server)
- [ ] All boards flashed with latest firmware (see Flash section below)

### Software
- [ ] ferrite-server running on host: `cargo run -p ferrite-server` (port 4000)
- [ ] ferrite-dashboard running on host: `cd ferrite-dashboard && dx serve` (port 8080)
- [ ] ferrite-gateway running on RPi with all transports:
  ```bash
  sudo RUST_LOG=info ferrite-gateway \
    --server http://<host-ip>:4000 \
    --usb-port /dev/ttyACM0 --usb-port /dev/ttyACM1 \
    --http-ingest-port 4001 --ble
  ```
- [ ] Dashboard accessible at `http://localhost:8080`
- [ ] Login with admin/admin (Basic auth default)

## Flash Firmware

### From host machine (probe-rs + espflash + nrfjprog)

```bash
# ESP32-C3 — edit .env with WiFi credentials + server IP first
cd examples/embassy-esp32c3
# Update .env: WIFI_SSID, WIFI_PASS, SERVER_HOST
espflash save-image --chip esp32c3 --merge target/.../release/embassy-esp32c3-example /tmp/esp.bin
# SCP to RPi and flash via esptool, or flash directly if ESP32 is on host USB

# STM32L4A6 — Embassy USB CDC
cd examples/embassy-stm32l4a6
cargo build --release
probe-rs download --chip STM32L4A6ZGTx target/thumbv7em-none-eabihf/release/embassy-stm32l4a6-example
probe-rs reset --chip STM32L4A6ZGTx

# STM32WL55 — RTIC LPUART (may need --connect-under-reset erase first)
cd examples/rtic-stm32wl55
cargo build --release
arm-none-eabi-objcopy -O binary target/thumbv7em-none-eabi/release/rtic-stm32wl55-example /tmp/wl55.bin
st-flash --connect-under-reset write /tmp/wl55.bin 0x8000000

# STM32H563 — Embassy Ethernet
cd examples/embassy-stm32h563
cargo build --release
probe-rs download --chip STM32H563ZITx target/thumbv8m.main-none-eabihf/release/embassy-stm32h563-example
probe-rs reset --chip STM32H563ZITx

# nRF5340-DK — Zephyr BLE (requires Zephyr workspace at ~/zephyrproject)
cd ~/zephyrproject
# App core
west build -b nrf5340dk/nrf5340/cpuapp /path/to/examples/zephyr-nrf5340
nrfjprog --program build/zephyr/zephyr.hex -f NRF53 --coprocessor CP_APPLICATION --sectorerase --reset
# Network core (BLE controller — flash once, survives app core reflash)
west build -b nrf5340dk/nrf5340/cpunet -d build_net zephyr/samples/bluetooth/hci_ipc
nrfjprog --program build_net/zephyr/zephyr.hex -f NRF53 --coprocessor CP_NETWORK --sectorerase --reset
```

### Recovering locked boards
```bash
# STM32WL55 (RDP level 1)
st-flash --connect-under-reset erase

# nRF5340 (APPROTECT)
nrfjprog --recover -f NRF53
```

## Demo Scenarios

### Scenario 1: Live Telemetry (2 min)

**Goal**: Show real-time data flowing from all 5 devices.

1. Open dashboard at `http://localhost:8080`
2. Navigate to **Devices** page — all 5 devices should appear with green "online" dots
3. Click any device → **Device Detail** page
4. Show **Metrics** tab — `loop_count` incrementing, `uptime_seconds` growing
5. Show **Fleet Overview** page — all devices on one screen

**Talking points**:
- 5 MCU architectures (RISC-V, 2× Cortex-M4, 2× Cortex-M33)
- 5 firmware stacks (Embassy ×3, RTIC, Zephyr C+FFI)
- 5 transports (WiFi, USB CDC, USART VCP, Ethernet, BLE)
- All data stored in SQLite, no cloud dependency
- ESP32-C3 bypasses gateway (direct WiFi POST)
- Other 4 route through RPi edge gateway

### Scenario 2: Fault Recovery (3 min)

**Goal**: Demonstrate crash detection and symbolicated stack traces.

1. Trigger a fault (press reset or inject via debugger)
2. Wait for device to reboot and re-register
3. Navigate to **Faults** page — show fault record with PC, LR, stack pointer
4. Upload ELF via **Settings** page for symbolication

### Scenario 3: Offline Resilience (2 min)

**Goal**: Show gateway buffering when server is temporarily down.

1. Stop server: `Ctrl+C` on the `cargo run -p ferrite-server` terminal
2. Wait 30s — devices keep sending, gateway buffers to SQLite
3. Restart server: `cargo run -p ferrite-server`
4. Watch dashboard — buffered data appears within seconds

### Scenario 4: Device Comparison (1 min)

1. Navigate to **Compare** view
2. Select `esp32c3-embassy-01` and `stm32wl55-rtic-01`
3. Show side-by-side `uptime_seconds` — note different reporting intervals

### Scenario 5: Transport Disconnect/Reconnect (1 min)

1. Unplug L4A6 USB cable — gateway detects disconnect
2. Dashboard shows device going stale
3. Replug — gateway auto-reconnects, data resumes

## Verification Commands

```bash
# Server health
curl -s http://localhost:4000/health

# List all devices with status
curl -s -u admin:admin http://localhost:4000/devices | python3 -m json.tool

# Check metrics for a device
curl -s -u admin:admin http://localhost:4000/devices/stm32wl55-rtic-01/metrics?limit=5

# RPi gateway logs
ssh pi@<rpi-ip> "tail -20 /tmp/ferrite-gateway.log"

# RPi USB devices
ssh pi@<rpi-ip> "lsusb && ls /dev/ttyACM*"

# Gateway HTTP ingest health
ssh pi@<rpi-ip> "curl -s http://localhost:4001/health"

# BLE scan (verify nRF5340 advertising)
ssh pi@<rpi-ip> "sudo hcitool lescan --duplicates"
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Device shows "UNKNOWN" status | Heartbeat not updating status | Server fix: `update_device_status_by_id()` on every heartbeat |
| "unknown" device in dashboard | Chunks without DeviceInfo context | Gateway batching (200ms window) groups chunks per upload |
| L4A6 USB CDC no data | DTR not set by gateway | `port.write_data_terminal_ready(true)` in gateway |
| WL55 no serial output | Using USART2 instead of LPUART1 | Switch to LPUART1 (AF8), use LPUART BRR formula |
| ESP32-C3 won't build | esp-hal version mismatch | Pin to esp-hal 0.23 ecosystem, nightly-2025-04-15 |
| nRF5340 bt_enable() hangs | Network core not programmed | Flash `hci_ipc` sample on cpunet |
| nRF5340 "debug port ID 0" | APPROTECT enabled | `nrfjprog --recover -f NRF53` |
| WL55 "Coprocessor access error" | Hard-float target, no FPU | Use `thumbv7em-none-eabi` (soft-float) |
| Embassy firmware hangs standalone | WFE wake issue without debugger | Use `raw::Executor` spin-poll loop |
| No LEDs on L4A6 | PB0 not connected to LD1 | Use PB14 (red) or PB7 (blue) instead |
| BLE device not discovered | Name not in ad packet | Move `BT_DATA_NAME_COMPLETE` to `ad[]` |
| H563 "BusFault" on boot | Wrong RCC_RSR address | H563 RCC base is `0x44020C00`, RSR at `+0x0D0` |
| H563 no DHCP | Direct ETH to RPi, no DHCP server | Use static IP (192.168.0.200/24) |
| Cross-compile OpenSSL error | reqwest native-tls | Use `rustls-tls` feature instead |
