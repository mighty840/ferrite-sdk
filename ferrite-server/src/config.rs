//! Configuration loaded from environment variables at startup.
//!
//! If `KEYCLOAK_URL`, `KEYCLOAK_REALM`, and `KEYCLOAK_CLIENT_ID` are set,
//! the server uses Keycloak OIDC authentication. Otherwise, it falls back
//! to HTTP Basic auth.

use serde::Serialize;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// AuthMode
// ---------------------------------------------------------------------------

/// Detected authentication mode based on environment variables.
#[derive(Debug)]
pub enum AuthMode {
    Keycloak(KeycloakConfig),
    Basic(BasicAuthConfig),
}

/// Keycloak OIDC configuration.
#[derive(Debug)]
pub struct KeycloakConfig {
    /// Base URL (e.g. `http://localhost:8080`).
    pub url: String,
    /// Realm name.
    pub realm: String,
    /// Public client ID for the dashboard SPA.
    pub client_id: String,
    /// Optional confidential client secret (for server-side token exchange).
    pub client_secret: Option<String>,
}

impl KeycloakConfig {
    /// OpenID Connect authorization endpoint.
    pub fn auth_endpoint(&self) -> String {
        format!(
            "{}/realms/{}/protocol/openid-connect/auth",
            self.url, self.realm
        )
    }

    /// OpenID Connect token endpoint.
    pub fn token_endpoint(&self) -> String {
        format!(
            "{}/realms/{}/protocol/openid-connect/token",
            self.url, self.realm
        )
    }

    /// OpenID Connect userinfo endpoint.
    pub fn userinfo_endpoint(&self) -> String {
        format!(
            "{}/realms/{}/protocol/openid-connect/userinfo",
            self.url, self.realm
        )
    }

    /// OpenID Connect JWKS endpoint for token verification.
    pub fn jwks_endpoint(&self) -> String {
        format!(
            "{}/realms/{}/protocol/openid-connect/certs",
            self.url, self.realm
        )
    }

    /// OpenID Connect logout endpoint.
    pub fn logout_endpoint(&self) -> String {
        format!(
            "{}/realms/{}/protocol/openid-connect/logout",
            self.url, self.realm
        )
    }
}

/// Basic HTTP authentication configuration.
#[derive(Debug)]
pub struct BasicAuthConfig {
    pub username: String,
    pub password: String,
}

// ---------------------------------------------------------------------------
// User roles
// ---------------------------------------------------------------------------

/// Role-based access control levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    /// Read-only dashboard access.
    Viewer = 0,
    /// Can register/provision devices but not delete or change settings.
    Provisioner = 1,
    /// Full access.
    Admin = 2,
}

impl UserRole {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "admin" => Self::Admin,
            "provisioner" => Self::Provisioner,
            _ => Self::Viewer,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Provisioner => "provisioner",
            Self::Viewer => "viewer",
        }
    }

    /// Check if this role can perform write operations (create/update).
    pub fn can_write(&self) -> bool {
        *self >= Self::Provisioner
    }

    /// Check if this role can perform destructive operations (delete, settings).
    pub fn can_admin(&self) -> bool {
        *self == Self::Admin
    }
}

/// Additional user accounts for basic auth mode.
/// Format: `BASIC_AUTH_USERS=user1:pass1:admin,user2:pass2:viewer`
#[derive(Debug, Clone)]
pub struct BasicAuthUser {
    pub username: String,
    pub password: String,
    pub role: UserRole,
}

/// Parse the BASIC_AUTH_USERS env var into a list of user accounts.
pub fn parse_basic_auth_users() -> Vec<BasicAuthUser> {
    let mut users = Vec::new();

    // Always include the primary admin user
    // (handled separately in validate_basic_auth)

    if let Ok(val) = std::env::var("BASIC_AUTH_USERS") {
        for entry in val.split(',') {
            let parts: Vec<&str> = entry.trim().split(':').collect();
            if parts.len() >= 2 {
                let role = if parts.len() >= 3 {
                    UserRole::parse(parts[2])
                } else {
                    UserRole::Viewer
                };
                users.push(BasicAuthUser {
                    username: parts[0].to_string(),
                    password: parts[1].to_string(),
                    role,
                });
            }
        }
    }

    users
}

// ---------------------------------------------------------------------------
// AuthConfig
// ---------------------------------------------------------------------------

/// Top-level auth configuration resolved from environment.
#[derive(Debug)]
pub struct AuthConfig {
    pub mode: AuthMode,
    /// Optional API key for device ingest endpoints.
    pub ingest_api_key: Option<String>,
    /// Optional CORS allowed origin.
    pub cors_origin: Option<String>,
    /// Data retention period in days (0 = disabled). Default: 90.
    pub retention_days: Option<u64>,
    /// Rate limit (requests per second per IP). 0 = disabled.
    pub rate_limit_rps: Option<f64>,
    /// Webhook URL for alerting (fault, device offline).
    pub alert_webhook_url: Option<String>,
    /// Minutes before a device is considered offline (default: 10).
    pub alert_offline_minutes: u64,
    /// 16-byte hex-encoded AES-128 key for chunk encryption (32 hex chars).
    /// If set, the server will decrypt chunks with the encrypted flag.
    pub chunk_encryption_key: Option<[u8; 16]>,
    /// Additional basic auth users with roles.
    pub additional_users: Vec<BasicAuthUser>,
}

impl AuthConfig {
    /// Build auth config from environment variables.
    ///
    /// Keycloak mode requires all three: `KEYCLOAK_URL`, `KEYCLOAK_REALM`,
    /// `KEYCLOAK_CLIENT_ID`. If any is missing, falls back to basic auth
    /// using `BASIC_AUTH_USER` / `BASIC_AUTH_PASS` (defaults: admin/admin).
    pub fn from_env() -> Self {
        let kc_url = optional_env("KEYCLOAK_URL");
        let kc_realm = optional_env("KEYCLOAK_REALM");
        let kc_client_id = optional_env("KEYCLOAK_CLIENT_ID");

        let mode = match (kc_url, kc_realm, kc_client_id) {
            (Some(url), Some(realm), Some(client_id)) => {
                tracing::info!("Auth mode: Keycloak ({})", url);
                AuthMode::Keycloak(KeycloakConfig {
                    url,
                    realm,
                    client_id,
                    client_secret: optional_env("KEYCLOAK_CLIENT_SECRET"),
                })
            }
            _ => {
                let username = optional_env("BASIC_AUTH_USER").unwrap_or_else(|| "admin".into());
                let password = optional_env("BASIC_AUTH_PASS").unwrap_or_else(|| "admin".into());
                tracing::info!("Auth mode: Basic (user={})", username);
                AuthMode::Basic(BasicAuthConfig { username, password })
            }
        };

        let chunk_encryption_key = optional_env("CHUNK_ENCRYPTION_KEY").and_then(|hex_str| {
            let bytes = hex::decode(&hex_str).ok()?;
            if bytes.len() == 16 {
                let mut key = [0u8; 16];
                key.copy_from_slice(&bytes);
                Some(key)
            } else {
                tracing::warn!(
                    "CHUNK_ENCRYPTION_KEY must be 32 hex chars (16 bytes), got {} chars",
                    hex_str.len()
                );
                None
            }
        });

        Self {
            mode,
            ingest_api_key: optional_env("INGEST_API_KEY"),
            cors_origin: optional_env("CORS_ORIGIN"),
            retention_days: optional_env("RETENTION_DAYS").and_then(|v| v.parse().ok()),
            rate_limit_rps: optional_env("RATE_LIMIT_RPS").and_then(|v| v.parse().ok()),
            alert_webhook_url: optional_env("ALERT_WEBHOOK_URL"),
            alert_offline_minutes: optional_env("ALERT_OFFLINE_MINUTES")
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            chunk_encryption_key,
            additional_users: parse_basic_auth_users(),
        }
    }
}

/// Response for `GET /auth/mode` — lets the dashboard discover auth config.
#[derive(Debug, Serialize)]
pub struct AuthModeResponse {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
}

impl AuthConfig {
    /// Build the discovery response for the dashboard.
    pub fn mode_response(&self) -> AuthModeResponse {
        match &self.mode {
            AuthMode::Keycloak(kc) => AuthModeResponse {
                mode: "keycloak".into(),
                authority: Some(format!("{}/realms/{}", kc.url, kc.realm)),
                client_id: Some(kc.client_id.clone()),
            },
            AuthMode::Basic(_) => AuthModeResponse {
                mode: "basic".into(),
                authority: None,
                client_id: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keycloak_endpoints() {
        let kc = KeycloakConfig {
            url: "https://auth.example.com".into(),
            realm: "ferrite".into(),
            client_id: "dashboard".into(),
            client_secret: None,
        };
        assert_eq!(
            kc.auth_endpoint(),
            "https://auth.example.com/realms/ferrite/protocol/openid-connect/auth"
        );
        assert_eq!(
            kc.token_endpoint(),
            "https://auth.example.com/realms/ferrite/protocol/openid-connect/token"
        );
        assert_eq!(
            kc.jwks_endpoint(),
            "https://auth.example.com/realms/ferrite/protocol/openid-connect/certs"
        );
    }

    #[test]
    fn mode_response_basic() {
        let config = AuthConfig {
            mode: AuthMode::Basic(BasicAuthConfig {
                username: "admin".into(),
                password: "admin".into(),
            }),
            ingest_api_key: None,
            cors_origin: None,
            retention_days: None,
            rate_limit_rps: None,
            alert_webhook_url: None,
            alert_offline_minutes: 10,
            chunk_encryption_key: None,
            additional_users: vec![],
        };
        let resp = config.mode_response();
        assert_eq!(resp.mode, "basic");
        assert!(resp.authority.is_none());
    }

    #[test]
    fn mode_response_keycloak() {
        let config = AuthConfig {
            mode: AuthMode::Keycloak(KeycloakConfig {
                url: "http://localhost:8080".into(),
                realm: "ferrite".into(),
                client_id: "dashboard".into(),
                client_secret: None,
            }),
            ingest_api_key: None,
            cors_origin: None,
            retention_days: None,
            rate_limit_rps: None,
            alert_webhook_url: None,
            alert_offline_minutes: 10,
            chunk_encryption_key: None,
            additional_users: vec![],
        };
        let resp = config.mode_response();
        assert_eq!(resp.mode, "keycloak");
        assert_eq!(
            resp.authority.as_deref(),
            Some("http://localhost:8080/realms/ferrite")
        );
    }
}
