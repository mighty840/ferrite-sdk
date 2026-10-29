# Security

ferrite-sdk and ferrite-server provide multiple layers of security: per-device credentials that authenticate every chunk, dual-mode authentication, role-based access control, and API key protection for ingest endpoints.

## Per-device credentials

By default every device sends the same `INGEST_API_KEY`, so anything holding that key (a device pulled from the field, a gateway) can report as any device, and its reports reach the [evidence log](/guide/evidence-log). A **device credential** is a key only one device has. The device seals every chunk with it (AES-128-CCM: encrypted and authenticated), and the server stores the chunk under the device the key belongs to.

**1. Issue a credential** (provisioner or admin). The key is in this response only:

```bash
curl -X POST -u admin:<password> https://ferrite.example.com/devices/plc-line3-07/credential
# {"device_id":"plc-line3-07","key_id":2895710042,"key":"5f1c…","rotated":false}
```

Calling it again rotates the key; the old one stops working at once. `DELETE` on the same path revokes it (admin). `GET` shows the key ID and when it was issued and last used, never the key. Each issue, rotation and revocation is recorded in the evidence log (`device.credential_set`, `device.credential_rotated`, `device.credential_revoked`).

**2. Give it to the device.**

- Linux (`ferrite-agent`): add a `[credential]` table to `agent.conf`, see [Linux agent](/guide/linux-agent#device-credential).
- MCU firmware: wrap the transport. `boot_epoch` must differ on every boot (a hardware RNG value, or a boot counter kept in flash), so that nonces never repeat:

```rust
use ferrite_sdk::encryption::{DeviceCredential, EncryptedTransport};

let credential = DeviceCredential { key_id: 2895710042, key: DEVICE_KEY };
let mut transport = EncryptedTransport::with_credential(transport, &credential, rng.next_u32());
```

The SDK's encoder keeps payloads small enough to seal (225 bytes with the `encryption` feature), and with a credential the transport never falls back to sending a chunk in clear.

**What the server checks:**

| Situation | Result |
|---|---|
| Chunk sealed with a known key, tag verifies, not seen before | Stored under the key's device |
| Wrong key, unknown or revoked key ID | Rejected |
| The same chunk again (replay) | Rejected |
| Sealed DeviceInfo naming a different device | That DeviceInfo is ignored |
| Unsealed chunks for a device that has a credential | Rejected |
| Unsealed chunks for a device without one | Accepted as before, unless `DEVICE_AUTH_REQUIRED=1` |

Rejections are listed in the ingest response and counted in `ferrite_auth_failures_total`. The devices list shows how each device's latest report authenticated (`last_auth`: `credential` or `none`), so you can see which devices still need a credential before you set `DEVICE_AUTH_REQUIRED=1`.

Gateways forward sealed chunks unchanged and never hold device keys. The server attributes each sealed chunk by its key, so a gateway batch that mixes several boards is split correctly.

**Replay protection** works per key and boot epoch: the server remembers the last 4096 counters of each of a device's 32 most recent boots, so chunks that arrive late or out of order (a gateway draining its buffer) are still accepted once. Anything older is rejected.

**Storage:** keys live in `credentials.db` next to the main database (`--credentials-db` to move it), created with mode `0600`. Retention and the demo reset don't touch it. It holds every device's key, so keep it private and back it up separately; `GET /admin/backup` covers only the main database.

## Shared-key encryption

`EncryptedTransport::with_shared_key` encrypts with one key for the whole fleet, which the server decrypts with `CHUNK_ENCRYPTION_KEY` (32 hex characters). It hides chunk contents on untrusted links (LoRa, BLE in public spaces) but says nothing about which device sent them: every device holding the key looks the same. Prefer device credentials. Pass a per-boot `boot_epoch` as above; `EncryptedTransport::new` (no epoch) repeats nonces after every reboot.

**Wire format:** see [Encrypted chunks](/reference/chunk-format#encrypted-chunks).

## Authentication

The server supports two authentication modes, selected automatically at startup:

### Keycloak OIDC

Set these environment variables to enable Keycloak:

```bash
KEYCLOAK_URL=https://keycloak.example.com
KEYCLOAK_REALM=ferrite
KEYCLOAK_CLIENT_ID=ferrite-dashboard
```

The server validates JWTs locally using JWKS (cached for 5 minutes), with fallback to the Keycloak userinfo endpoint. The dashboard uses Authorization Code flow with PKCE.

### Basic auth

Without Keycloak variables, the server falls back to HTTP Basic auth:

```bash
BASIC_AUTH_USER=admin      # default: admin
BASIC_AUTH_PASS=s3cret     # required: the server won't start without it
```

`BASIC_AUTH_PASS=admin` works for local development; the server logs a warning.

## Multi-user RBAC

Four roles control what authenticated users can do:

| Role | Read | Create/Update | Delete | Admin paths | Evidence log |
|---|---|---|---|---|---|
| **Auditor** | Evidence log only | No | No | No | Yes |
| **Viewer** | Yes | No | No | No | No |
| **Provisioner** | Yes | Yes | No | No | No |
| **Admin** | Yes | Yes | Yes | Yes | Yes |

Admin paths include `/admin/*`, `/groups/*`, and `/ota/*`. The [evidence log](/guide/evidence-log) (`/evidence/*`) is readable by admins and auditors only. The auditor role is meant for an external auditor: it can read the evidence log and nothing else.

### Adding users (Basic auth)

```bash
BASIC_AUTH_USERS="viewer1:password1:viewer,provisioner1:password2:provisioner,auditor1:password3:auditor"
```

Format: `username:password:role` pairs separated by commas.

### Adding roles (Keycloak)

Create realm roles `ferrite-admin`, `ferrite-provisioner` and `ferrite-auditor` in Keycloak and assign them to users. A user with several gets the strongest (admin > provisioner > auditor); users with none are viewers.

## API key protection

Protect ingest endpoints with an API key:

```bash
INGEST_API_KEY=my-secret-key
```

Devices include the key in the `X-API-Key` header. The `/ingest/elf` endpoint always requires authentication: the API key, or a provisioner or admin user. Viewers and auditors get `403`.

## Rate limiting

Protect against abuse with per-IP rate limiting:

```bash
RATE_LIMIT_RPS=100   # requests per second per IP
```

Applied to `/ingest/*` and `/auth/*` endpoints. Burst allowance is 10x the RPS value.
