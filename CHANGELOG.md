# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/). Before 1.0, minor versions may
contain breaking changes.

## [Unreleased]

### Added
- **Per-device credentials** (#118): `POST /devices/:id/credential` issues a device its own AES-128-CCM key (rotate by calling again; `DELETE` revokes; issue, rotation and revocation go into the evidence log). Devices seal every chunk with it (`EncryptedTransport::with_credential` in the SDK, `[credential]` in `ferrite-agent`), and the server stores each sealed chunk under the device its key belongs to, rejecting forged, replayed and revoked ones. Unsealed reports for a device that has a credential are rejected; `DEVICE_AUTH_REQUIRED=1` rejects all unsealed reports. Devices show how their latest report authenticated (`last_auth`). Keys live in `credentials.db` (`--credentials-db`, mode `0600`), which retention and the demo reset leave alone.
- SDK: `EncryptedTransport` also implements `AsyncChunkTransport`; `ChunkSealer` seals chunks for hosts that build request bodies themselves.
- `examples/qemu-fault`: a HardFault recorded in retained RAM, read back after the reset and symbolicated with `addr2line`, on QEMU's lm3s6965 without hardware. CI runs it.
### Changed (breaking)
- In Basic auth mode, `ferrite-server serve` refuses to start without `BASIC_AUTH_PASS` instead of falling back to `admin`/`admin`. Set `BASIC_AUTH_PASS=admin` for local development; the server logs a warning for that password.
- `/events/stream` (live server-sent events) now requires a signed-in user; it was public. The dashboard doesn't use it.
- `POST /ingest/elf` with user auth now needs the provisioner or admin role; viewers get `403`. Uploaded symbols decide how every crash report reads. The ingest API key still works.
- C API: the transport callback typedefs are now `ferrite_send_chunk_fn_t` and `ferrite_is_available_fn_t` (were `IotaiSendChunkFn` / `IotaiIsAvailableFn`), matching the other `ferrite_*_t` types. The Rust-side `ferrite-ffi` types drop the `Iotai` prefix too (`FerriteError`, `FerriteTransport`, …). Regenerate your header with cbindgen; the `ferrite_*_t` structs and `FERRITE_ERROR_T_*` codes are unchanged.
- The CLI crate is now `ferrite-ctl` (directory `ferrite-ctl/`); the binary is still `ferrite`. `ferrite-cli` is taken on crates.io (#79).
- **Auditor role:** `auditor` (basic auth) / `ferrite-auditor` (Keycloak) reads the evidence log and nothing else. The evidence log (`/evidence/*`) is now limited to admins and auditors; viewers and provisioners lose access.

### Fixed
- SDK: when the retained block's magic is wrong (first power-on, or RAM holding something else), `init()` now clears the whole block, as the docs describe. It used to reset only the header, so power-up noise under the fault record's `valid` flag could be reported as a HardFault with a random PC (#136).
- Server: ELF and firmware uploads accept only versions made of letters, digits, `.`, `-`, `_` and `+` (1 to 64 characters, starting with a letter or digit), since the version names the stored file. Other values get `400`.
- **Chunk encryption reused nonces:** the AES-CCM nonce was a counter that restarted at 0 on every boot, under the same key, which breaks CCM's confidentiality and integrity. The nonce now carries a per-boot epoch (`with_credential`, `with_shared_key`); `EncryptedTransport::new` keeps the old nonce and is documented as such.
- Chunk encryption could panic: a 226- or 227-byte payload passed the size check but overflowed the 256-byte chunk buffer. Chunks too large to seal are now refused before writing, and with the `encryption` feature the encoder keeps payloads at 225 bytes so every chunk can be sealed. With a device credential, a chunk is never sent in clear.
- Docs: the CCM tag is 8 bytes, not 16 (sealing adds 21 bytes).
- Gateway: the BLE scanner retries with backoff when it can't start (e.g. at boot, before BlueZ is ready) or when BlueZ goes away, instead of stopping for good until the gateway restarts. The Pi unit is ordered after `bluetooth.service`.
- Gateway: `GATEWAY_BUFFER_DB` and `USB_BAUD` had no effect, because the `--buffer-db` and `--usb-baud` defaults always overrode them. A flag now wins only when given. If you set `GATEWAY_BUFFER_DB` (the Pi deployment does), the gateway now opens that file instead of `ferrite-gateway.db` in its working directory; let it drain its buffer, or move the old file, before upgrading.
- Gateway udev rules (`deploy/rpi-gateway/99-ferrite.rules`) gave every local user read/write access to the boards' serial ports (`0666`), so any user could inject chunks. Ports now belong to group `dialout`, mode `0660`.
- Docs: the gateway's environment variables are `INGEST_API_KEY`, `USB_PORTS` and `USB_BAUD`, not `FERRITE_API_KEY`, `FERRITE_USB_PORT` and `FERRITE_USB_BAUD`.
- `ChunkEncoder::encode_device_info` could index past its buffer when the device ID and firmware version were both near 127 bytes; the version is now shortened to fit.

## [0.1.0] — 2026-10-18 (pre-release)

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
- **Update-reach reports:** devices per firmware build, campaign reach (on target, unreachable, time to patch) and per-device firmware history, as JSON or CSV (`/reports/*`).
- **Evidence log:** an append-only, hash-chained record of firmware uploads, OTA targets, the campaign lifecycle and device update outcomes, in its own `evidence.db` that retention never purges; `GET /evidence`, `GET /evidence/verify`, `--evidence-db`.

### Dashboard (`ferrite-dashboard`, Dioxus/WASM)
- Fleet overview and topology, devices, faults, crashes, metrics charts, device comparison, CSV/JSON export, OTA campaigns and firmware.
- Deep links and reloads (e.g. `/ota/campaigns/3`) work when the server serves the dashboard; the web manifest and icon ship with the build, and the unused service worker is gone.

### CLI (`ferrite`)
- Devices, faults, groups, metrics, OTA targets.
- `ferrite ota keygen | sign | verify | upload` for signed firmware.
- `ferrite report builds | campaign | history` for update-reach reports.
- `ferrite evidence list | verify`; CSV output quotes fields that need it.
- `--since` on `faults` and `metrics` uses the server's timestamp format; same-day rows were filtered out before.

### Gateway (`ferrite-gateway`)
- Bridges USB serial, BLE GATT and HTTP-ingest devices to the server with offline buffering.
- Reconnects USB serial ports after a reset/replug and BLE devices after a dropped or silent connection.
- Relays device OTA reads (target, firmware, manifest) to the server with the gateway's credentials.

### Linux agent (`ferrite-agent`)
- Reports embedded Linux devices with the same chunk format as the MCU SDK: image identity from `/etc/os-release`, CPU/memory/disk/temperature, watched systemd units (e.g. the CODESYS runtime), failed units, core dumps, and reboot reasons.
- Offline spool, unprivileged hardened systemd unit, optional TLS (`tls` feature).

### Examples
- Embassy: nRF52840, nRF5340, RP2040, RP235x, ESP32-C3, STM32L4A6 (USB CDC), STM32WL55, STM32H563 (Ethernet).
- RTIC: STM32F4, STM32WL55. C: STM32L4A6 (bare metal), nRF5340 (Zephyr).
- STM32H563: embassy-boot bootloader + signed OTA app with watchdog-driven rollback (`FERRITE_BUILD_ID`, `FERRITE_FIRMWARE_VERSION`, `FERRITE_OTA_PUBKEY`, `FERRITE_SECURITY_VERSION`).

### Build and release
- `Cargo.lock` files are committed and CI builds with `--locked`.
- Releases publish `libferrite_ffi.a` for Cortex-M targets, the C header, Linux x86_64 binaries for server/gateway/CLI, `SHA256SUMS`, and a version-tagged server image (`ghcr.io/mighty840/ferrite:<tag>`).
- QEMU integration tests (Cortex-M3) run in CI, including manifest verification.
- Docker image: `DB_RESET_INTERVAL` (demo reset) defaults to off, and when set it restarts the server around the reset so the reset takes effect; uploaded firmware and ELF files live under the mounted `data/` directory.

### Known limitations
- Not published to crates.io yet: the `ferrite` and `ferrite-cli` names are taken there (#79).
- The C API still uses the `Iotai*` prefix for its two callback typedefs (renamed in the next release).
- OTA for BLE devices (nRF5340/MCUboot) is not done yet (#92).
