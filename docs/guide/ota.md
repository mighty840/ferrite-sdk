# Signed OTA updates

Every firmware image ferrite delivers is described by a **signed manifest**. The manifest is signed offline with an Ed25519 key; devices hold only the public key. Neither the server nor the network can make a device install an image the key holder didn't sign, and devices refuse to roll back below a minimum security version.

```
build ─▶ ferrite ota sign (offline key) ─▶ ferrite ota upload ─▶ server stores image + manifest
                                                                       │
device ◀── verify manifest (public key) ◀── GET /ota/firmware/:id/download (X-Firmware-Manifest)
   └─ stream image into the update slot, check size + SHA-256, then mark bootable
```

## 1. Create a signing key

```bash
ferrite ota keygen --out ota.key
```

This writes `ota.key` (secret, mode `0600`) and `ota.key.pub`, and prints the public key as a server setting and as a Rust constant for the firmware. Keep `ota.key` offline or in your CI's secret store, and never commit it.

## 2. Trust the key on the server

```bash
OTA_TRUSTED_KEYS=<hex public key>[,<hex public key>...]
```

With trusted keys set, the server rejects any upload without a manifest, with a manifest signed by an unknown key, or with a manifest that doesn't match the uploaded bytes. A malformed key stops the server at startup instead of falling back to unchecked uploads.

Without `OTA_TRUSTED_KEYS` the server accepts unsigned uploads (and logs a warning), but devices still refuse any image without a valid manifest.

## 3. Sign and upload a release

```bash
# Raw image as the bootloader expects it (not the ELF)
arm-none-eabi-objcopy -O binary target/thumbv7em-none-eabihf/release/app app.bin

ferrite ota sign --key ota.key --firmware app.bin \
  --version 1.4.0 --build-id 0x5f3a9c21 --security-version 2
# → app.bin.manifest

ferrite ota verify --manifest app.bin.manifest --firmware app.bin --pubkey ota.key.pub
ferrite ota upload --firmware app.bin          # picks up app.bin.manifest
```

`--build-id` must be the build ID the new firmware reports, so devices and campaigns can tell builds apart. Version, build ID and signer of the stored artifact come from the signed manifest, not from request headers.

## 4. Verify on the device

Enable the `http` feature (it includes `ota-manifest`) and pass an `UpdatePolicy` to the OTA client:

```rust
use ferrite_sdk::ota::{OtaCheckResult, OtaClient};
use ferrite_sdk::ota_manifest::UpdatePolicy;

// From `ferrite ota keygen`. Ship more than one key to rotate keys safely.
const OTA_TRUSTED_KEYS: [[u8; 32]; 1] = [[/* ... */]];
// Bump with releases that fix vulnerabilities; older images are then refused.
const SECURITY_VERSION: u32 = 2;

let policy = UpdatePolicy {
    trusted_keys: &OTA_TRUSTED_KEYS,
    min_security_version: SECURITY_VERSION,
};

let ota = OtaClient::new(DEVICE_ID, BUILD_ID, "http://192.168.0.103:4001", None);

// One TCP connection per request: responses aren't drained.
if let OtaCheckResult::UpdateAvailable(target) = ota.check(&mut connect().await?).await? {
    let manifest = ota
        .download(&mut connect().await?, &target, &policy, |offset, data| {
            update_slot.write(offset, data)
        })
        .await?;
    // Only now: mark the update slot bootable (MCUboot / embassy-boot) and reboot.
}
```

`download()` checks, in order:

1. The `X-Firmware-Manifest` header is present and well-formed. There is no unsigned fallback.
2. The signature is valid for one of `trusted_keys` (matched by key ID).
3. `security_version >= min_security_version` (anti-rollback).
4. The manifest names the target's `build_id`.
5. While streaming: the image never exceeds `image_size`, so nothing beyond it is written to flash.
6. At the end: exact size and SHA-256 match.

Verification is `no_std` and allocation-free. On a Cortex-M3 it adds about 11 KB of flash (`ed25519-compact` built with `opt_size`); signing code is never linked into firmware.

Checks 1–4 run before the first byte is written to flash. The hash is only known at the end, so **mark the slot bootable only after `download()` returns `Ok`**.

Devices authenticate to `GET /ota/targets/:device_id` and the download with `X-API-Key` when the server has an `INGEST_API_KEY`. Without one, these routes require user authentication.

### Devices behind a gateway

Devices that only reach `ferrite-gateway` (for example over a boards-only Ethernet link) point `OtaClient` at the gateway's HTTP ingest port. The gateway relays exactly three reads to the server with its own credentials: `GET /ota/targets/<device_id>`, `GET /ota/firmware/<id>/download` and `GET /ota/firmware/<id>/manifest`. Everything else gets a 404. The gateway passes `X-Firmware-Manifest` through unchanged; it is not trusted with integrity, since the device verifies the signature itself.

## Example: STM32H563 with embassy-boot

`examples/embassy-stm32h563-bootloader` and `examples/embassy-stm32h563` are a complete bootloader + app pair for the Nucleo-H563ZI. The app polls the gateway's OTA relay, writes verified images to the update slot, and confirms them after a self-test.

**Flash layout** (both crates' `memory.x`, 8 KB sectors):

| Partition | Address | Size |
|---|---|---|
| Bootloader | `0x08000000` | 48 KB |
| Bootloader state | `0x0800C000` | 8 KB |
| ACTIVE (app) | `0x08010000` | 256 KB, bank 1 |
| DFU (update slot) | `0x08100000` | 264 KB, bank 2 |

Keeping DFU in bank 2 lets the app write it while running from bank 1.

**Build-time inputs** (read by the app's `build.rs`):

| Variable | Meaning |
|---|---|
| `FERRITE_OTA_PUBKEY` | Required. Trusted public key(s): hex or path to `ota.key.pub`, comma-separated |
| `FERRITE_BUILD_ID` | Build ID the firmware reports; pass the same value to `ferrite ota sign --build-id`. Defaults to `SOURCE_DATE_EPOCH`, then the build time |
| `FERRITE_SECURITY_VERSION` | Anti-rollback floor compiled into the firmware (default 0) |

**First flash** (bootloader + app, once, over the probe):

```bash
cd examples/embassy-stm32h563-bootloader && cargo build --release
probe-rs download --chip STM32H563ZITx target/thumbv8m.main-none-eabihf/release/embassy-stm32h563-bootloader

cd ../embassy-stm32h563
FERRITE_OTA_PUBKEY=../../ota.key.pub FERRITE_BUILD_ID=1001 cargo build --release
probe-rs download --chip STM32H563ZITx target/thumbv8m.main-none-eabihf/release/embassy-stm32h563-example
probe-rs reset --chip STM32H563ZITx
```

**Release over the air:**

```bash
FERRITE_OTA_PUBKEY=ota.key.pub FERRITE_BUILD_ID=1002 cargo build --release
arm-none-eabi-objcopy -O binary target/thumbv8m.main-none-eabihf/release/embassy-stm32h563-example app.bin
ferrite ota sign --key ota.key --firmware app.bin --version 0.2.0 --build-id 1002 --security-version 0
ferrite ota upload --firmware app.bin
# then create and activate a campaign for the device (dashboard or API)
```

**Confirm and rollback.** After an update the bootloader swaps the new image in and boots it with the state "swap". The app confirms (`mark_booted`) only after Ethernet is up and one upload through the gateway succeeded. Until then any reset swaps the old image back. The app runs an independent watchdog (20 s), so a hung or crashing image resets itself and is rolled back without anyone touching the board. Each upload carries an OTA-request chunk with the build ID, so the server marks the device `installed` in its campaign once the new build reports in.

To see a rollback, build an update with `FERRITE_TEST_HANG_AFTER_BOOT=1` (test builds only): it hangs before confirming, the watchdog fires, and the board comes back on the previous build.

## Rollback protection

`build_id` is a timestamp or hash and has no order, so rollback protection uses a separate counter, `security_version`. A device compiled with `SECURITY_VERSION = 3` rejects every manifest below 3, including validly signed old releases with known vulnerabilities. Increase it in releases that fix security issues.

## Key rotation

Devices accept any key in `trusted_keys`. To rotate: ship firmware that trusts both the old and new key, sign later releases with the new key, then drop the old key from firmware and `OTA_TRUSTED_KEYS`.

## Manifest format

168 bytes, little-endian. The signature covers `"ferrite-ota-manifest-v1" || body`.

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | magic `FOTA` |
| 4 | 1 | format version (`1`) |
| 5 | 3 | reserved (0) |
| 8 | 8 | `build_id` (u64) |
| 16 | 4 | `security_version` (u32) |
| 20 | 4 | `image_size` (u32) |
| 24 | 32 | SHA-256 of the image |
| 56 | 32 | version string, UTF-8, zero-padded |
| 88 | 8 | key ID: first 8 bytes of SHA-256(public key) |
| 96 | 8 | reserved (0) |
| 104 | 64 | Ed25519 signature |

The server sends it hex-encoded in the `X-Firmware-Manifest` header and as raw bytes from `GET /ota/firmware/:id/manifest`.
