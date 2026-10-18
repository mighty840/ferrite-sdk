# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/). Before 1.0, minor versions may
contain breaking changes.

## [Unreleased]

## [0.1.0] — unreleased (pre-release)

First tagged release. Firmware observability for Cortex-M and RISC-V MCUs,
with a server, dashboard, CLI, edge gateway, and signed OTA updates.

### SDK (`ferrite-sdk`, `no_std`, no alloc)
- Fault capture (HardFault frame, registers, stack snapshot) in retained RAM, reboot reasons, metrics (counter/gauge/histogram), trace buffer, binary chunk encoding with CRC-16, optional AES-128-CCM chunk encryption and compression.
- Transports: UART, USB CDC (embassy-usb), HTTP (reqwless, WiFi/Ethernet), LoRa (SX126x/SX127x), BLE (`ferrite-ble-nrf`).
- Runtime integrations: Embassy (`ferrite-embassy`), RTIC (`ferrite-rtic`), C/FreeRTOS/Zephyr via `ferrite-ffi` (static library + cbindgen header).
- **Signed OTA:** Ed25519-signed 168-byte update manifests, verified on the device before any flash write; anti-rollback via `security_version`; streaming size and SHA-256 check; fail closed (`ota-manifest`, `http` features). Verification adds about 11 KB of flash on Cortex-M3.
- `OtaClient` takes one connection per request and sends origin-form requests with a `Host` header.
- OTA-request chunks carry a flags byte; `OTA_FLAG_ROLLED_BACK` reports a bootloader rollback.
- The `embassy` feature no longer pins `embassy-time`/`embassy-sync`.

### Server (`ferrite-server`)
- Chunk ingest, device registry and registration, faults with ELF symbolication, crash deduplication and analytics, metrics, reboots, device groups and tags.
- Auth: Keycloak OIDC or Basic, roles (admin/viewer/provisioner), optional ingest API key, rate limiting, CORS.
- Retention policy, database backup/export, Prometheus metrics, SSE live updates, webhook alerts.
- OTA: firmware artifacts, per-device targets, campaigns (groups/tags, rollout percent, pause/rollback, failure threshold).
- **Signed OTA:** `OTA_TRUSTED_KEYS` rejects uploads without a valid manifest signed by a trusted key; manifests are stored and served in `X-Firmware-Manifest` and `GET /ota/firmware/:id/manifest`. Devices can read their OTA target and firmware with the ingest API key.
- Rollback reports remove the failed build's target, mark the device `failed` and pause a campaign over its failure threshold.

### Dashboard (`ferrite-dashboard`, Dioxus/WASM)
- Fleet overview and topology, devices, faults, crashes, metrics charts, device comparison, CSV/JSON export, OTA campaigns and firmware.

### CLI (`ferrite`)
- Devices, faults, groups, metrics, OTA targets.
- `ferrite ota keygen | sign | verify | upload` for signed firmware.

### Gateway (`ferrite-gateway`)
- Bridges USB serial, BLE GATT and HTTP-ingest devices to the server with offline buffering.
- Reconnects USB serial ports after a reset/replug and BLE devices after a dropped or silent connection.
- Relays device OTA reads (target, firmware, manifest) to the server with the gateway's credentials.

### Examples
- Embassy: nRF52840, nRF5340, RP2040, RP235x, ESP32-C3, STM32L4A6 (USB CDC), STM32WL55, STM32H563 (Ethernet).
- RTIC: STM32F4, STM32WL55. C: STM32L4A6 (bare metal), nRF5340 (Zephyr).
- STM32H563: embassy-boot bootloader + signed OTA app with watchdog-driven rollback (`FERRITE_BUILD_ID`, `FERRITE_OTA_PUBKEY`, `FERRITE_SECURITY_VERSION`).

### Build and release
- `Cargo.lock` files are committed and CI builds with `--locked`.
- Releases publish `libferrite_ffi.a` for Cortex-M targets, the C header, Linux x86_64 binaries for server/gateway/CLI, `SHA256SUMS`, and a version-tagged server image (`ghcr.io/mighty840/ferrite:<tag>`).
- QEMU integration tests (Cortex-M3) run in CI, including manifest verification.

### Known limitations
- Not published to crates.io yet: the `ferrite` and `ferrite-cli` names are taken there (#79).
- The C API still uses the `Iotai*` type prefix from the project's former name.
- OTA for BLE devices (nRF5340/MCUboot) is not done yet (#92).
