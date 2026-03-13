# Security

ferrite-sdk and ferrite-server provide multiple layers of security: chunk encryption on the wire, dual-mode authentication, role-based access control, and API key protection for ingest endpoints.

## Chunk encryption (AES-128-CCM)

The SDK includes an `EncryptedTransport<T>` wrapper that encrypts chunk payloads using AES-128-CCM before transmission. This protects data on untrusted transports (LoRa, BLE in public spaces).

```rust
use ferrite_sdk::encryption::EncryptedTransport;

// 16-byte key (AES-128)
let key = [0x01, 0x02, /* ... */ 0x10];
let encrypted = EncryptedTransport::new(transport, key);
```

**Wire format:** encrypted chunks set `FLAG_ENCRYPTED` (0x04) in the header. The payload becomes `nonce (13 bytes) || ciphertext || tag (16 bytes)`.

**Server-side decryption:** set the `CHUNK_ENCRYPTION_KEY` environment variable (32-char hex string) on the server. Encrypted chunks are decrypted transparently before processing.

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
BASIC_AUTH_PASS=s3cret     # default: admin
```

## Multi-user RBAC

Three role levels control what authenticated users can do:

| Role | Read | Create/Update | Delete | Admin paths |
|---|---|---|---|---|
| **Viewer** | Yes | No | No | No |
| **Provisioner** | Yes | Yes | No | No |
| **Admin** | Yes | Yes | Yes | Yes |

Admin paths include `/admin/*`, `/groups/*`, and `/ota/*`.

### Adding users (Basic auth)

```bash
BASIC_AUTH_USERS="viewer1:password1:viewer,provisioner1:password2:provisioner"
```

Format: `username:password:role` pairs separated by commas.

### Adding roles (Keycloak)

Create realm roles `ferrite-admin` and `ferrite-provisioner` in Keycloak and assign them to users. Users without either role default to Viewer.

## API key protection

Protect ingest endpoints with an API key:

```bash
INGEST_API_KEY=my-secret-key
```

Devices include the key in the `X-API-Key` header. The `/ingest/elf` endpoint always requires authentication (API key or user auth).

## Rate limiting

Protect against abuse with per-IP rate limiting:

```bash
RATE_LIMIT_RPS=100   # requests per second per IP
```

Applied to `/ingest/*` and `/auth/*` endpoints. Burst allowance is 10x the RPS value.
