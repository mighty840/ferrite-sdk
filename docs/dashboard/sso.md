# SSO / Keycloak Setup

For production deployments, you can protect the iotai dashboard and server API with single sign-on (SSO) using Keycloak.

## Overview

The setup involves:

1. Running a Keycloak instance
2. Creating a realm and client for the iotai dashboard
3. Configuring a reverse proxy (e.g., nginx or Caddy) to validate tokens
4. Updating the dashboard to redirect to Keycloak for login

## Keycloak configuration

### Create a realm

1. Log in to the Keycloak admin console (e.g., `http://keycloak:8080/admin`)
2. Create a new realm called `iotai`
3. Under the realm settings, configure:
   - Display name: "iotai Device Platform"
   - Login theme: your preference

### Create a client

1. In the `iotai` realm, go to Clients and create a new client:
   - Client ID: `iotai-dashboard`
   - Client type: OpenID Connect
   - Root URL: `https://dashboard.example.com`
2. Configure the client:
   - Valid redirect URIs: `https://dashboard.example.com/*`
   - Web origins: `https://dashboard.example.com`
   - Client authentication: Off (public client for SPA)
3. Note the client ID for use in the dashboard configuration.

### Create users

Create users in the `iotai` realm and assign them roles as needed. For a simple setup, a single `viewer` role is sufficient.

## Reverse proxy configuration

### Caddy example

```caddyfile
dashboard.example.com {
    reverse_proxy localhost:8080
}

api.example.com {
    # Validate JWT tokens from Keycloak
    @authenticated {
        header Authorization Bearer*
    }

    handle @authenticated {
        reverse_proxy localhost:4000
    }

    handle {
        respond "Unauthorized" 401
    }
}
```

### nginx example

```nginx
server {
    listen 443 ssl;
    server_name api.example.com;

    location / {
        # Validate JWT using lua-resty-openidc or oauth2-proxy
        auth_request /auth;
        proxy_pass http://127.0.0.1:4000;
    }
}
```

For a more robust setup, consider using [oauth2-proxy](https://oauth2-proxy.github.io/oauth2-proxy/) as an authentication middleware between the reverse proxy and the iotai-server.

## Dashboard OIDC configuration

The dashboard uses the standard OpenID Connect authorization code flow with PKCE. Configure the following values in the dashboard settings:

| Setting | Value |
|---|---|
| OIDC Authority | `https://keycloak.example.com/realms/iotai` |
| Client ID | `iotai-dashboard` |
| Redirect URI | `https://dashboard.example.com/callback` |
| Scope | `openid profile` |

## Device authentication

For device-to-server authentication (the chunk upload path), API keys or mTLS are more appropriate than OIDC. Consider:

- **API keys**: Add an `X-API-Key` header to chunk uploads and validate it in the reverse proxy or a server middleware.
- **mTLS**: Use client certificates for device authentication. Each device gets a unique certificate signed by your CA.

The iotai-server itself does not currently implement authentication. Use a reverse proxy or API gateway for access control.
