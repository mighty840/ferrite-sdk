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
// AuthConfig
// ---------------------------------------------------------------------------

/// Top-level auth configuration resolved from environment.
#[derive(Debug)]
pub struct AuthConfig {
    pub mode: AuthMode,
    /// Optional API key for device ingest endpoints.
    pub ingest_api_key: Option<String>,
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

        Self {
            mode,
            ingest_api_key: optional_env("INGEST_API_KEY"),
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
        };
        let resp = config.mode_response();
        assert_eq!(resp.mode, "keycloak");
        assert_eq!(
            resp.authority.as_deref(),
            Some("http://localhost:8080/realms/ferrite")
        );
    }
}
