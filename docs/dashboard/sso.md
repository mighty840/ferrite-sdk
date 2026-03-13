# SSO / Keycloak Setup

The ferrite platform supports single sign-on via Keycloak OIDC. The server handles JWT validation natively -- no reverse proxy auth is needed.

## Overview

1. Run a Keycloak instance
2. Create a realm, client, and roles
3. Set environment variables on the ferrite-server
4. The dashboard auto-detects OIDC mode and shows an SSO login button

## Server configuration

Set these environment variables (in `.env` or your deployment config):

```bash
KEYCLOAK_URL=https://keycloak.example.com
KEYCLOAK_REALM=ferrite
KEYCLOAK_CLIENT_ID=ferrite-dashboard
```

The server auto-detects Keycloak mode when all three variables are set. Without them, it falls back to Basic auth.

## Keycloak configuration

### Create a realm

1. Log in to the Keycloak admin console
2. Create a new realm called `ferrite`

### Create a client

1. In the `ferrite` realm, go to Clients and create a new client:
   - Client ID: `ferrite-dashboard`
   - Client type: OpenID Connect
   - Root URL: `https://dashboard.example.com`
2. Configure the client:
   - Valid redirect URIs: `https://dashboard.example.com/*`
   - Web origins: `https://dashboard.example.com`
   - Client authentication: Off (public client for SPA)

### Create roles

Create these **realm roles** for role-based access control:

| Role | Permissions |
|---|---|
| `ferrite-admin` | Full access: read, write, delete, admin paths |
| `ferrite-provisioner` | Read + create/update devices and groups |
| *(no role)* | Viewer: read-only access |

Assign roles to users in the Keycloak admin console.

### Create users

Create users in the `ferrite` realm and assign them the appropriate roles.

## How it works

1. The dashboard calls `GET /auth/mode` on startup and detects `keycloak` mode
2. User clicks "Sign in with SSO" -- the dashboard redirects to Keycloak's authorization endpoint with PKCE
3. After authentication, Keycloak redirects back with an authorization code
4. The dashboard exchanges the code for tokens via the Keycloak token endpoint
5. The access token (JWT) is stored in `sessionStorage` and sent as `Authorization: Bearer <token>` on every API call
6. The server validates the JWT using cached JWKS keys (refreshed every 5 minutes), falling back to the userinfo endpoint if JWKS validation fails
7. Roles are extracted from the `realm_access.roles` JWT claim

## Device authentication

For device-to-server authentication (chunk upload), use API keys rather than OIDC:

```bash
# Server-side
INGEST_API_KEY=your-secret-device-key

# Device-side (in firmware or gateway config)
X-API-Key: your-secret-device-key
```

The `/ingest/elf` endpoint additionally accepts user auth (Bearer or Basic) for manual ELF uploads.
