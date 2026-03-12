//! Authentication logic for both Keycloak and Basic auth modes.

use axum::http::header;
use base64::Engine;
use std::sync::Mutex;
use std::time::Instant;

use crate::config::{AuthConfig, AuthMode, BasicAuthConfig, KeycloakConfig};

// ---------------------------------------------------------------------------
// Authenticated user identity
// ---------------------------------------------------------------------------

/// Claims extracted from a validated token or basic auth credentials.
/// Fields are read by downstream extractors (e.g. route handlers pulling from request extensions).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UserClaims {
    pub sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
}

// ---------------------------------------------------------------------------
// Basic auth
// ---------------------------------------------------------------------------

/// Validate a Basic auth `Authorization` header value.
///
/// Returns `Ok(UserClaims)` if credentials match, `Err(reason)` otherwise.
pub fn validate_basic_auth(
    auth_header: &str,
    config: &BasicAuthConfig,
) -> Result<UserClaims, &'static str> {
    let encoded = auth_header
        .strip_prefix("Basic ")
        .ok_or("invalid Basic auth header")?;

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "invalid base64")?;

    let credentials = String::from_utf8(decoded).map_err(|_| "invalid utf-8 in credentials")?;

    let (user, pass) = credentials
        .split_once(':')
        .ok_or("missing ':' in credentials")?;

    if user == config.username && pass == config.password {
        Ok(UserClaims {
            sub: user.to_string(),
            email: None,
            name: Some(user.to_string()),
        })
    } else {
        Err("invalid credentials")
    }
}

// ---------------------------------------------------------------------------
// JWKS cache for local JWT validation
// ---------------------------------------------------------------------------

/// Cached JWKS key set with TTL-based refresh.
struct JwksCache {
    keys: Vec<jsonwebtoken::jwk::Jwk>,
    fetched_at: Instant,
}

/// Global JWKS cache — refreshed every 5 minutes.
static JWKS_CACHE: Mutex<Option<JwksCache>> = Mutex::new(None);
const JWKS_TTL_SECS: u64 = 300;

/// Fetch (or return cached) JWKS keys from the Keycloak certs endpoint.
async fn get_jwks_keys(config: &KeycloakConfig) -> Result<Vec<jsonwebtoken::jwk::Jwk>, String> {
    // Check cache first
    {
        let cache = JWKS_CACHE.lock().unwrap();
        if let Some(ref c) = *cache {
            if c.fetched_at.elapsed().as_secs() < JWKS_TTL_SECS {
                return Ok(c.keys.clone());
            }
        }
    }

    // Fetch fresh JWKS
    let url = config.jwks_endpoint();
    tracing::debug!("Fetching JWKS from {}", url);
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("JWKS fetch failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("JWKS fetch failed: HTTP {}", resp.status()));
    }

    let jwks: jsonwebtoken::jwk::JwkSet = resp
        .json()
        .await
        .map_err(|e| format!("JWKS parse failed: {e}"))?;

    let keys = jwks.keys;

    // Update cache
    {
        let mut cache = JWKS_CACHE.lock().unwrap();
        *cache = Some(JwksCache {
            keys: keys.clone(),
            fetched_at: Instant::now(),
        });
    }

    Ok(keys)
}

/// JWT claims we extract from Keycloak tokens.
#[derive(Debug, serde::Deserialize)]
struct KeycloakClaims {
    sub: Option<String>,
    email: Option<String>,
    preferred_username: Option<String>,
    name: Option<String>,
}

// ---------------------------------------------------------------------------
// Keycloak JWT validation
// ---------------------------------------------------------------------------

/// Validate a Keycloak Bearer token using local JWKS verification.
/// Falls back to the userinfo endpoint if JWKS validation fails.
pub async fn validate_keycloak_token(
    token: &str,
    config: &KeycloakConfig,
) -> Result<UserClaims, String> {
    // Try local JWKS-based validation first
    if let Ok(keys) = get_jwks_keys(config).await {
        // Decode the JWT header to find the key ID
        let header =
            jsonwebtoken::decode_header(token).map_err(|e| format!("invalid JWT header: {e}"))?;

        let kid = header.kid.as_deref();

        // Find matching key
        for jwk in &keys {
            if let Some(ref jwk_kid) = jwk.common.key_id {
                if kid.is_some() && kid != Some(jwk_kid.as_str()) {
                    continue;
                }
            }

            let decoding_key = match jsonwebtoken::DecodingKey::from_jwk(jwk) {
                Ok(k) => k,
                Err(_) => continue,
            };

            let mut validation = jsonwebtoken::Validation::new(header.alg);
            // Keycloak issuer is {url}/realms/{realm}
            let issuer = format!("{}/realms/{}", config.url, config.realm);
            validation.set_issuer(&[&issuer]);
            // Don't validate audience — Keycloak tokens may have different audiences
            validation.validate_aud = false;

            match jsonwebtoken::decode::<KeycloakClaims>(token, &decoding_key, &validation) {
                Ok(data) => {
                    return Ok(UserClaims {
                        sub: data.claims.sub.unwrap_or_else(|| "unknown".into()),
                        email: data.claims.email,
                        name: data.claims.preferred_username.or(data.claims.name),
                    });
                }
                Err(e) => {
                    tracing::debug!("JWKS validation failed for key: {e}");
                    continue;
                }
            }
        }
        // All keys failed — fall through to userinfo
        tracing::debug!("No JWKS key matched, falling back to userinfo");
    }

    // Fallback: validate via userinfo endpoint
    let client = reqwest::Client::new();
    let resp = client
        .get(config.userinfo_endpoint())
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("userinfo request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("token validation failed: HTTP {}", resp.status()));
    }

    let info: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse userinfo: {}", e))?;

    Ok(UserClaims {
        sub: info["sub"].as_str().unwrap_or("unknown").to_string(),
        email: info["email"].as_str().map(String::from),
        name: info["preferred_username"]
            .as_str()
            .or(info["name"].as_str())
            .map(String::from),
    })
}

// ---------------------------------------------------------------------------
// Unified validation
// ---------------------------------------------------------------------------

/// Extract and validate credentials from an HTTP request's Authorization header.
pub async fn validate_request(
    auth_header: Option<&str>,
    config: &AuthConfig,
) -> Result<UserClaims, AuthError> {
    let header_val = auth_header.ok_or(AuthError::Missing)?;

    match &config.mode {
        AuthMode::Basic(basic) => {
            validate_basic_auth(header_val, basic).map_err(|_| AuthError::Invalid)
        }
        AuthMode::Keycloak(_kc) => {
            let token = header_val
                .strip_prefix("Bearer ")
                .ok_or(AuthError::Invalid)?;
            validate_keycloak_token(token, _kc)
                .await
                .map_err(|_| AuthError::Invalid)
        }
    }
}

/// Validate an optional API key for ingest endpoints.
pub fn validate_ingest_api_key(
    api_key_header: Option<&str>,
    required_key: &str,
) -> Result<(), AuthError> {
    match api_key_header {
        Some(key) if key == required_key => Ok(()),
        Some(_) => Err(AuthError::Invalid),
        None => Err(AuthError::Missing),
    }
}

#[derive(Debug)]
pub enum AuthError {
    Missing,
    Invalid,
}

impl AuthError {
    pub fn www_authenticate_header(config: &AuthConfig) -> &'static str {
        match &config.mode {
            AuthMode::Basic(_) => "Basic realm=\"ferrite\"",
            AuthMode::Keycloak(_) => "Bearer realm=\"ferrite\"",
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: extract Authorization header from request
// ---------------------------------------------------------------------------

pub fn extract_auth_header(headers: &header::HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
}

pub fn extract_api_key_header(headers: &header::HeaderMap) -> Option<&str> {
    headers.get("X-API-Key").and_then(|v| v.to_str().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_auth_valid() {
        let config = BasicAuthConfig {
            username: "admin".into(),
            password: "secret".into(),
        };
        // "admin:secret" base64 = "YWRtaW46c2VjcmV0"
        let result = validate_basic_auth("Basic YWRtaW46c2VjcmV0", &config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().sub, "admin");
    }

    #[test]
    fn basic_auth_invalid_password() {
        let config = BasicAuthConfig {
            username: "admin".into(),
            password: "secret".into(),
        };
        // "admin:wrong" base64 = "YWRtaW46d3Jvbmc="
        let result = validate_basic_auth("Basic YWRtaW46d3Jvbmc=", &config);
        assert!(result.is_err());
    }

    #[test]
    fn basic_auth_missing_prefix() {
        let config = BasicAuthConfig {
            username: "admin".into(),
            password: "secret".into(),
        };
        let result = validate_basic_auth("Bearer some-token", &config);
        assert!(result.is_err());
    }
}
