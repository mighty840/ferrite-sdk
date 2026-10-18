//! Integration tests for ferrite-server.
//!
//! These tests start a real server on a random port and exercise the HTTP API.
//! Tests that need Keycloak are gated behind the `keycloak` feature and
//! expect a running Keycloak instance (see docker-compose.yml).

use reqwest::{Client, StatusCode};
use std::net::TcpListener;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Find a free TCP port on localhost.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Spawn a ferrite-server on a random port, returning the base URL.
/// Configures auth from environment (falls back to Basic admin/admin).
async fn spawn_server() -> String {
    spawn_server_with_env(vec![]).await
}

/// All env keys that AuthConfig::from_env() reads.
const AUTH_ENV_KEYS: &[&str] = &[
    "KEYCLOAK_URL",
    "KEYCLOAK_REALM",
    "KEYCLOAK_CLIENT_ID",
    "KEYCLOAK_CLIENT_SECRET",
    "BASIC_AUTH_USER",
    "BASIC_AUTH_PASS",
    "INGEST_API_KEY",
    "CORS_ORIGIN",
    "RETENTION_DAYS",
    "RATE_LIMIT_RPS",
    "ALERT_WEBHOOK_URL",
    "ALERT_OFFLINE_MINUTES",
    "CHUNK_ENCRYPTION_KEY",
    "BASIC_AUTH_USERS",
    "OTA_TRUSTED_KEYS",
];

/// Spawn a ferrite-server on a random port with additional env vars set.
/// Clears all auth-related env vars first to avoid cross-test contamination.
async fn spawn_server_with_env(env_overrides: Vec<(&str, &str)>) -> String {
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let base_url = format!("http://{addr}");

    // Convert to owned strings so they can be moved into the spawned task.
    let owned_overrides: Vec<(String, String)> = env_overrides
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    // Clear all auth-related env vars, set overrides, then build config.
    // Tests MUST run with --test-threads=1 since env vars are process-global.
    for key in AUTH_ENV_KEYS {
        std::env::remove_var(key);
    }
    for (k, v) in &owned_overrides {
        std::env::set_var(k, v);
    }

    let config: &'static ferrite_server::config::AuthConfig =
        Box::leak(Box::new(ferrite_server::config::AuthConfig::from_env()));

    let addr_clone = addr.clone();
    tokio::spawn(async move {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let elf_dir = tmp.path().join("elfs");
        std::fs::create_dir_all(&elf_dir).unwrap();

        let store = ferrite_server::store::Store::open(&db_path).unwrap();
        let symbolicator = ferrite_server::symbolicate::Symbolicator::new(None, elf_dir.clone());

        let (event_tx, _) = tokio::sync::broadcast::channel(256);
        let state = Arc::new(ferrite_server::AppState {
            store: Mutex::new(store),
            symbolicator: Mutex::new(symbolicator),
            elf_dir: elf_dir.clone(),
            firmware_dir: elf_dir,
            config,
            event_tx,
            counters: ferrite_server::prometheus::RequestCounters::new(),
            rate_limiter: None,
            evidence: ferrite_server::evidence::EvidenceLog::open_in_memory().unwrap(),
        });

        let app = ferrite_server::ingest::router(state);
        let listener = tokio::net::TcpListener::bind(&addr_clone).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });

    // Wait for server to be ready
    let client = Client::new();
    for _ in 0..50 {
        if client
            .get(format!("{base_url}/auth/mode"))
            .send()
            .await
            .is_ok()
        {
            return base_url;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("server did not start within 2.5s");
}

fn basic_auth_header() -> String {
    use base64::Engine;
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode("admin:admin")
    )
}

// =========================================================================
// Basic auth tests
// =========================================================================

#[tokio::test]
async fn auth_mode_returns_basic_by_default() {
    let base = spawn_server().await;
    let resp = Client::new()
        .get(format!("{base}/auth/mode"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["mode"], "basic");
    assert!(body.get("authority").is_none() || body["authority"].is_null());
}

#[tokio::test]
async fn devices_require_auth() {
    let base = spawn_server().await;
    let client = Client::new();

    // No auth → 401
    let resp = client.get(format!("{base}/devices")).send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Wrong auth → 401
    let resp = client
        .get(format!("{base}/devices"))
        .header("Authorization", "Basic d3Jvbmc6Y3JlZHM=")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Correct auth → 200
    let resp = client
        .get(format!("{base}/devices"))
        .header("Authorization", basic_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// =========================================================================
// ELF upload auth tests
// =========================================================================

#[tokio::test]
async fn elf_upload_requires_auth() {
    let base = spawn_server().await;
    let client = Client::new();

    // No auth → 401
    let resp = client
        .post(format!("{base}/ingest/elf"))
        .header("Content-Type", "application/octet-stream")
        .body(b"fake elf".to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn elf_upload_with_basic_auth_succeeds() {
    let base = spawn_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{base}/ingest/elf"))
        .header("Content-Type", "application/octet-stream")
        .header("Authorization", basic_auth_header())
        .header("X-Firmware-Version", "test-v1")
        .body(b"fake elf".to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["ok"], true);
}

#[tokio::test]
async fn elf_upload_with_api_key_succeeds() {
    let base = spawn_server_with_env(vec![("INGEST_API_KEY", "test-secret-key")]).await;
    let client = Client::new();

    let resp = client
        .post(format!("{base}/ingest/elf"))
        .header("Content-Type", "application/octet-stream")
        .header("X-API-Key", "test-secret-key")
        .header("X-Firmware-Version", "test-v2")
        .body(b"fake elf".to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// =========================================================================
// ELF size limit
// =========================================================================

#[tokio::test]
async fn elf_upload_rejects_oversized_payload() {
    let base = spawn_server().await;
    let client = Client::new();

    // 51 MB payload — exceeds 50 MB limit
    let big_payload = vec![0u8; 51 * 1024 * 1024];
    let resp = client
        .post(format!("{base}/ingest/elf"))
        .header("Content-Type", "application/octet-stream")
        .header("Authorization", basic_auth_header())
        .header("X-Firmware-Version", "test-big")
        .body(big_payload)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

// =========================================================================
// Ingest API key gating
// =========================================================================

#[tokio::test]
async fn ingest_chunks_require_api_key_when_configured() {
    let base = spawn_server_with_env(vec![("INGEST_API_KEY", "device-secret")]).await;
    let client = Client::new();

    // No API key → 401
    let resp = client
        .post(format!("{base}/ingest/chunks"))
        .header("Content-Type", "application/octet-stream")
        .body(vec![])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Wrong API key → 401
    let resp = client
        .post(format!("{base}/ingest/chunks"))
        .header("Content-Type", "application/octet-stream")
        .header("X-API-Key", "wrong-key")
        .body(vec![])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Correct API key → 200
    let resp = client
        .post(format!("{base}/ingest/chunks"))
        .header("Content-Type", "application/octet-stream")
        .header("X-API-Key", "device-secret")
        .body(vec![])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn ingest_chunks_open_when_no_api_key_configured() {
    let base = spawn_server().await;
    let client = Client::new();

    // No API key required → 200
    let resp = client
        .post(format!("{base}/ingest/chunks"))
        .header("Content-Type", "application/octet-stream")
        .body(vec![])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// =========================================================================
// CORS tests
// =========================================================================

#[tokio::test]
async fn cors_preflight_returns_correct_headers() {
    let base = spawn_server_with_env(vec![("CORS_ORIGIN", "http://localhost:8080")]).await;
    let client = Client::new();

    let resp = client
        .request(reqwest::Method::OPTIONS, format!("{base}/ingest/chunks"))
        .header("Origin", "http://localhost:8080")
        .header("Access-Control-Request-Method", "POST")
        .header(
            "Access-Control-Request-Headers",
            "content-type,authorization",
        )
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let allow_origin = resp
        .headers()
        .get("access-control-allow-origin")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(allow_origin, "http://localhost:8080");

    let allow_methods = resp
        .headers()
        .get("access-control-allow-methods")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(allow_methods.contains("POST"));
}

// =========================================================================
// Health endpoint
// =========================================================================

#[tokio::test]
async fn health_is_public() {
    let base = spawn_server().await;
    let resp = Client::new()
        .get(format!("{base}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// =========================================================================
// Prometheus metrics endpoint (#33)
// =========================================================================

#[tokio::test]
async fn prometheus_metrics_is_public() {
    let base = spawn_server().await;
    let resp = Client::new()
        .get(format!("{base}/metrics/prometheus"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.text().await.unwrap();
    assert!(body.contains("ferrite_devices_total"));
    assert!(body.contains("ferrite_faults_total"));
    assert!(body.contains("ferrite_ingest_requests_total"));
}

// =========================================================================
// SSE endpoint (#26)
// =========================================================================

#[tokio::test]
async fn sse_endpoint_is_public() {
    let base = spawn_server().await;
    // Just verify the endpoint accepts connections (returns 200 with SSE content type)
    let resp = Client::new()
        .get(format!("{base}/events/stream"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("text/event-stream"));
}

// =========================================================================
// Device groups (#27)
// =========================================================================

#[tokio::test]
async fn groups_crud() {
    let base = spawn_server().await;
    let client = Client::new();
    let auth = basic_auth_header();

    // Create group
    let resp = client
        .post(format!("{base}/groups"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "name": "fleet-a", "description": "Test fleet" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: serde_json::Value = resp.json().await.unwrap();
    let group_id = body["group"]["id"].as_i64().unwrap();
    assert_eq!(body["group"]["name"], "fleet-a");

    // List groups
    let resp = client
        .get(format!("{base}/groups"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["groups"].as_array().unwrap().len(), 1);

    // Get single group
    let resp = client
        .get(format!("{base}/groups/{group_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Update group
    let resp = client
        .put(format!("{base}/groups/{group_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "name": "fleet-b" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["group"]["name"], "fleet-b");

    // Delete group
    let resp = client
        .delete(format!("{base}/groups/{group_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn groups_require_auth() {
    let base = spawn_server().await;
    let resp = Client::new()
        .get(format!("{base}/groups"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// Admin endpoints (#40, #29)
// =========================================================================

#[tokio::test]
async fn admin_backup_requires_auth() {
    let base = spawn_server().await;
    let resp = Client::new()
        .get(format!("{base}/admin/backup"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_backup_returns_sqlite() {
    let base = spawn_server().await;
    let resp = Client::new()
        .get(format!("{base}/admin/backup"))
        .header("Authorization", basic_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.bytes().await.unwrap();
    assert!(bytes.starts_with(b"SQLite format 3\0"));
}

#[tokio::test]
async fn admin_retention_info() {
    let base = spawn_server().await;
    let resp = Client::new()
        .get(format!("{base}/admin/retention"))
        .header("Authorization", basic_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["retention_days"].is_number());
    assert!(body["enabled"].is_boolean());
}

// =========================================================================
// Keycloak integration tests (requires running Keycloak)
// =========================================================================

/// Keycloak config for tests. In CI, Keycloak runs on :8080 (service container).
/// Locally, docker-compose maps to :9090. Override via FERRITE_TEST_KEYCLOAK_URL.
#[cfg(feature = "keycloak-tests")]
fn keycloak_env() -> (String, String, String) {
    // Use a test-specific env var that is NOT in AUTH_ENV_KEYS (so it won't be cleared).
    let url = std::env::var("FERRITE_TEST_KEYCLOAK_URL")
        .or_else(|_| std::env::var("KEYCLOAK_URL"))
        .unwrap_or_else(|_| "http://localhost:8080".into());
    let realm = "ferrite".to_string();
    let client_id = "ferrite-dashboard".to_string();
    (url, realm, client_id)
}

/// Get an access token from Keycloak using the direct access grant (password flow).
#[cfg(feature = "keycloak-tests")]
async fn keycloak_token(
    keycloak_url: &str,
    realm: &str,
    client_id: &str,
    username: &str,
    password: &str,
) -> String {
    let client = Client::new();

    let resp = client
        .post(format!(
            "{keycloak_url}/realms/{realm}/protocol/openid-connect/token"
        ))
        .form(&[
            ("grant_type", "password"),
            ("client_id", client_id),
            ("username", username),
            ("password", password),
            ("scope", "openid profile email"),
        ])
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "Keycloak token request failed"
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    body["access_token"].as_str().unwrap().to_string()
}

#[cfg(feature = "keycloak-tests")]
#[tokio::test]
async fn auth_mode_returns_keycloak_when_configured() {
    let (kc_url, kc_realm, kc_client) = keycloak_env();
    let base = spawn_server_with_env(vec![
        ("KEYCLOAK_URL", &kc_url),
        ("KEYCLOAK_REALM", &kc_realm),
        ("KEYCLOAK_CLIENT_ID", &kc_client),
    ])
    .await;

    let resp = Client::new()
        .get(format!("{base}/auth/mode"))
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["mode"], "keycloak");
    assert_eq!(body["authority"], format!("{kc_url}/realms/{kc_realm}"));
    assert_eq!(body["client_id"], kc_client);
}

#[cfg(feature = "keycloak-tests")]
#[tokio::test]
async fn keycloak_bearer_token_grants_access() {
    let (kc_url, kc_realm, kc_client) = keycloak_env();
    let token = keycloak_token(&kc_url, &kc_realm, &kc_client, "testuser", "testpass").await;

    let base = spawn_server_with_env(vec![
        ("KEYCLOAK_URL", &kc_url),
        ("KEYCLOAK_REALM", &kc_realm),
        ("KEYCLOAK_CLIENT_ID", &kc_client),
    ])
    .await;

    let client = Client::new();
    let resp = client
        .get(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[cfg(feature = "keycloak-tests")]
#[tokio::test]
async fn keycloak_invalid_token_rejected() {
    let (kc_url, kc_realm, kc_client) = keycloak_env();
    let base = spawn_server_with_env(vec![
        ("KEYCLOAK_URL", &kc_url),
        ("KEYCLOAK_REALM", &kc_realm),
        ("KEYCLOAK_CLIENT_ID", &kc_client),
    ])
    .await;

    let client = Client::new();
    let resp = client
        .get(format!("{base}/devices"))
        .header("Authorization", "Bearer invalid-token-here")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[cfg(feature = "keycloak-tests")]
#[tokio::test]
async fn keycloak_elf_upload_with_bearer_token() {
    let (kc_url, kc_realm, kc_client) = keycloak_env();
    let token = keycloak_token(&kc_url, &kc_realm, &kc_client, "testuser", "testpass").await;

    let base = spawn_server_with_env(vec![
        ("KEYCLOAK_URL", &kc_url),
        ("KEYCLOAK_REALM", &kc_realm),
        ("KEYCLOAK_CLIENT_ID", &kc_client),
    ])
    .await;

    let client = Client::new();
    let resp = client
        .post(format!("{base}/ingest/elf"))
        .header("Content-Type", "application/octet-stream")
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Firmware-Version", "kc-test-v1")
        .body(b"fake elf via keycloak".to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// =========================================================================
// OTA target tests (#31)
// =========================================================================

#[tokio::test]
async fn ota_targets_crud() {
    let base = spawn_server().await;
    let client = Client::new();
    let auth = basic_auth_header();

    // Initially empty
    let resp = client
        .get(format!("{base}/ota/targets"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["targets"].as_array().unwrap().is_empty());

    // Create a target
    let resp = client
        .post(format!("{base}/ota/targets"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "device_id": "dev-001",
            "target_version": "2.0.0",
            "target_build_id": 42,
            "firmware_url": "https://example.com/fw.bin"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["target"]["target_version"], "2.0.0");

    // Get specific target
    let resp = client
        .get(format!("{base}/ota/targets/dev-001"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["target"]["target_build_id"], 42);

    // Delete target
    let resp = client
        .delete(format!("{base}/ota/targets/dev-001"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Should be 404 now
    let resp = client
        .get(format!("{base}/ota/targets/dev-001"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ota_targets_require_auth() {
    let base = spawn_server().await;
    let client = Client::new();

    let resp = client
        .get(format!("{base}/ota/targets"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// Signed OTA manifests (#75)
// =========================================================================

mod signed_ota {
    use super::*;
    use ed25519_compact::{KeyPair, Seed};
    use ferrite_sdk::ota_manifest::{key_id, signing_message, Manifest, SignedManifest};
    use sha2::{Digest, Sha256};

    const IMAGE: &[u8] = &[0x42; 256];

    fn keypair(seed: u8) -> KeyPair {
        KeyPair::from_seed(Seed::new([seed; 32]))
    }

    fn manifest_hex(kp: &KeyPair, image: &[u8], build_id: u64) -> String {
        let mut version = heapless::String::new();
        version.push_str("3.1.0").unwrap();
        let manifest = Manifest {
            build_id,
            security_version: 2,
            image_size: image.len() as u32,
            sha256: Sha256::digest(image).into(),
            version,
            key_id: key_id(&kp.pk),
        };
        let body = manifest.encode_body();
        let sig = kp.sk.sign(signing_message(&body), None);
        let signed = SignedManifest::new(body, *sig).unwrap();
        String::from_utf8(signed.to_hex().to_vec()).unwrap()
    }

    async fn upload(
        base: &str,
        manifest: Option<&str>,
        image: &[u8],
    ) -> (StatusCode, serde_json::Value) {
        let mut req = Client::new()
            .post(format!("{base}/ota/firmware"))
            .header("Authorization", basic_auth_header())
            .header("X-Firmware-Version", "header-version")
            .body(image.to_vec());
        if let Some(m) = manifest {
            req = req.header("X-Firmware-Manifest", m);
        }
        let resp = req.send().await.unwrap();
        let status = resp.status();
        (status, resp.json().await.unwrap())
    }

    async fn spawn_with_trusted(kp: &KeyPair, extra: Vec<(&str, &str)>) -> String {
        let key = hex::encode(*kp.pk);
        let mut env = vec![("OTA_TRUSTED_KEYS", key.as_str())];
        env.extend(extra);
        spawn_server_with_env(env).await
    }

    #[tokio::test]
    async fn signed_upload_is_stored_and_served() {
        let kp = keypair(1);
        let base = spawn_with_trusted(&kp, vec![]).await;
        let manifest = manifest_hex(&kp, IMAGE, 77);

        let (status, body) = upload(&base, Some(&manifest), IMAGE).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let artifact = &body["artifact"];
        // Metadata comes from the signed manifest, not the free-text header.
        assert_eq!(artifact["version"], "3.1.0");
        assert_eq!(artifact["build_id"], 77);
        assert_eq!(artifact["security_version"], 2);
        assert_eq!(artifact["signer"], hex::encode(key_id(&kp.pk)));
        let id = artifact["id"].as_i64().unwrap();

        let client = Client::new();
        let resp = client
            .get(format!("{base}/ota/firmware/{id}/download"))
            .header("Authorization", basic_auth_header())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()["x-firmware-manifest"].to_str().unwrap(),
            manifest
        );
        assert_eq!(resp.bytes().await.unwrap().as_ref(), IMAGE);

        let resp = client
            .get(format!("{base}/ota/firmware/{id}/manifest"))
            .header("Authorization", basic_auth_header())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(hex::encode(resp.bytes().await.unwrap()), manifest);
    }

    #[tokio::test]
    async fn unsigned_upload_rejected_when_keys_configured() {
        let kp = keypair(1);
        let base = spawn_with_trusted(&kp, vec![]).await;
        let (status, body) = upload(&base, None, IMAGE).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"]
            .as_str()
            .unwrap()
            .contains("signed manifest required"));
    }

    #[tokio::test]
    async fn manifest_from_untrusted_key_rejected() {
        let trusted = keypair(1);
        let attacker = keypair(2);
        let base = spawn_with_trusted(&trusted, vec![]).await;
        let (status, _) = upload(&base, Some(&manifest_hex(&attacker, IMAGE, 1)), IMAGE).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn manifest_for_other_image_rejected() {
        let kp = keypair(1);
        let base = spawn_with_trusted(&kp, vec![]).await;
        let manifest = manifest_hex(&kp, IMAGE, 1);
        let (status, body) = upload(&base, Some(&manifest), &[0x43; 256]).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("does not match"));
    }

    #[tokio::test]
    async fn malformed_manifest_rejected() {
        let kp = keypair(1);
        let base = spawn_with_trusted(&kp, vec![]).await;
        let (status, _) = upload(&base, Some("deadbeef"), IMAGE).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unsigned_upload_allowed_without_trusted_keys() {
        let base = spawn_server().await;
        let (status, body) = upload(&base, None, IMAGE).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["artifact"]["version"], "header-version");
        assert!(body["artifact"]["manifest"].is_null());
    }

    #[tokio::test]
    async fn device_reads_ota_with_ingest_key_only() {
        let kp = keypair(1);
        let base = spawn_with_trusted(&kp, vec![("INGEST_API_KEY", "device-key")]).await;
        let (_, body) = upload(&base, Some(&manifest_hex(&kp, IMAGE, 5)), IMAGE).await;
        let id = body["artifact"]["id"].as_i64().unwrap();
        let client = Client::new();

        let get = |path: String, key: Option<&'static str>| {
            let mut req = client.get(format!("{base}{path}"));
            if let Some(k) = key {
                req = req.header("X-API-Key", k);
            }
            req.send()
        };

        let ok = get(format!("/ota/firmware/{id}/download"), Some("device-key"))
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK);
        assert!(ok.headers().contains_key("x-firmware-manifest"));

        let wrong = get(format!("/ota/firmware/{id}/download"), Some("nope"))
            .await
            .unwrap();
        assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

        let none = get(format!("/ota/firmware/{id}/download"), None)
            .await
            .unwrap();
        assert_eq!(none.status(), StatusCode::UNAUTHORIZED);

        // The key only opens the device-facing reads, not management routes.
        let list = get("/ota/firmware".into(), Some("device-key"))
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn device_ota_reads_need_user_auth_without_ingest_key() {
        let kp = keypair(1);
        let base = spawn_with_trusted(&kp, vec![]).await;
        let (_, body) = upload(&base, Some(&manifest_hex(&kp, IMAGE, 5)), IMAGE).await;
        let id = body["artifact"]["id"].as_i64().unwrap();
        let resp = Client::new()
            .get(format!("{base}/ota/firmware/{id}/download"))
            .header("X-API-Key", "anything")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}

// =========================================================================
// OTA rollback reports (#76)
// =========================================================================

#[tokio::test]
async fn rolled_back_device_is_failed_and_no_longer_offered_the_build() {
    use ferrite_sdk::chunks::encoder::{ChunkEncoder, OTA_FLAG_ROLLED_BACK};

    let base = spawn_server().await;
    let client = Client::new();
    let auth = basic_auth_header();

    let post_chunks = |chunks: Vec<u8>| {
        client
            .post(format!("{base}/ingest/chunks"))
            .header("Content-Type", "application/octet-stream")
            .body(chunks)
            .send()
    };
    let mut enc = ChunkEncoder::new();
    let mut body = Vec::new();
    enc.encode_device_info("dev-rb", "2.0.0", 200, |c| body.extend_from_slice(c));
    assert_eq!(post_chunks(body).await.unwrap().status(), StatusCode::OK);

    // Unsigned firmware is fine here (no OTA_TRUSTED_KEYS configured).
    let fw: serde_json::Value = client
        .post(format!("{base}/ota/firmware"))
        .header("Authorization", &auth)
        .header("X-Firmware-Version", "3.0.0")
        .header("X-Build-Id", "300")
        .body(vec![0xAB; 64])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let fw_id = fw["artifact"]["id"].as_i64().unwrap();
    let campaign: serde_json::Value = client
        .post(format!("{base}/ota/campaigns"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({"name": "v3", "firmware_id": fw_id, "target_version": "3.0.0"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cid = campaign["campaign"]["id"].as_i64().unwrap();
    client
        .post(format!("{base}/ota/campaigns/{cid}/activate"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    let target = client
        .get(format!("{base}/ota/targets/dev-rb"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(target.status(), StatusCode::OK);

    // The device comes back on build 200 and reports the rollback.
    let mut body = Vec::new();
    enc.encode_device_info("dev-rb", "2.0.0", 200, |c| body.extend_from_slice(c));
    enc.encode_ota_request_with_flags(200, 256, OTA_FLAG_ROLLED_BACK, |c| {
        body.extend_from_slice(c)
    });
    assert_eq!(post_chunks(body).await.unwrap().status(), StatusCode::OK);

    let target = client
        .get(format!("{base}/ota/targets/dev-rb"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(
        target.status(),
        StatusCode::NOT_FOUND,
        "failed build still offered"
    );

    let summary: serde_json::Value = client
        .get(format!("{base}/ota/campaigns/{cid}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(summary["campaign"]["failed"], 1);
    // 1 of 1 failed exceeds the default 5% threshold.
    assert_eq!(summary["campaign"]["campaign"]["status"], "paused");

    // Both the rollback and the automatic pause are in the evidence log.
    let evidence: serde_json::Value = client
        .get(format!("{base}/evidence?subject=device:dev-rb"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let kinds: Vec<&str> = evidence["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        vec!["device.first_seen", "device.update_rolled_back"]
    );
    let paused: serde_json::Value = client
        .get(format!("{base}/evidence?kind=campaign.auto_paused"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(paused["entries"][0]["subject"], format!("campaign:{cid}"));
}

// =========================================================================
// Evidence log (#77)
// =========================================================================

#[tokio::test]
async fn evidence_log_records_release_and_update_story() {
    use ferrite_sdk::chunks::encoder::ChunkEncoder;

    let base = spawn_server().await;
    let client = Client::new();
    let auth = basic_auth_header();
    let mut enc = ChunkEncoder::new();
    let mut report = |version: &str, build: u64| {
        let mut body = Vec::new();
        enc.encode_device_info("ev-1", version, build, |c| body.extend_from_slice(c));
        enc.encode_ota_request(build, 256, |c| body.extend_from_slice(c));
        client
            .post(format!("{base}/ingest/chunks"))
            .header("Content-Type", "application/octet-stream")
            .body(body)
            .send()
    };
    report("1.0.0", 100).await.unwrap();
    report("1.0.0", 100).await.unwrap(); // unchanged: no new entry

    let fw: serde_json::Value = client
        .post(format!("{base}/ota/firmware"))
        .header("Authorization", &auth)
        .header("X-Firmware-Version", "2.0.0")
        .header("X-Build-Id", "200")
        .body(vec![9; 32])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let campaign: serde_json::Value = client
        .post(format!("{base}/ota/campaigns"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "fix", "firmware_id": fw["artifact"]["id"], "target_version": "2.0.0"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cid = campaign["campaign"]["id"].as_i64().unwrap();
    client
        .post(format!("{base}/ota/campaigns/{cid}/activate"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    report("2.0.0", 200).await.unwrap();
    report("2.0.0", 200).await.unwrap(); // installed only once

    let body: serde_json::Value = client
        .get(format!("{base}/evidence"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let entries = body["entries"].as_array().unwrap();
    let kinds: Vec<&str> = entries
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        vec![
            "device.first_seen",
            "firmware.uploaded",
            "campaign.created",
            "campaign.activated",
            "device.build_changed",
            "device.update_installed",
            "campaign.completed",
        ]
    );
    assert!(body["chain_valid"].as_bool().unwrap());
    assert_eq!(body["head_seq"], 7);
    // User actions carry the user, device events the device.
    assert_ne!(entries[1]["actor"], "api");
    assert_eq!(entries[4]["actor"], "device:ev-1");
    assert_eq!(entries[4]["details"]["from_build_id"], 100);
    assert_eq!(entries[3]["details"]["target_build_id"], 200);

    let filtered: serde_json::Value = client
        .get(format!(
            "{base}/evidence?kind=campaign&subject=campaign:{cid}"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(filtered["entries"].as_array().unwrap().len(), 3);

    let verify: serde_json::Value = client
        .get(format!("{base}/evidence/verify"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(verify["valid"], true);
    assert_eq!(verify["entries"], 7);

    let anon = Client::new()
        .get(format!("{base}/evidence"))
        .send()
        .await
        .unwrap();
    assert_eq!(anon.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// Update-reach reports (#78)
// =========================================================================

#[tokio::test]
async fn update_reach_reports_json_and_csv() {
    use ferrite_sdk::chunks::encoder::ChunkEncoder;

    let base = spawn_server().await;
    let client = Client::new();
    let auth = basic_auth_header();
    let mut enc = ChunkEncoder::new();
    let mut report = |device: &str, version: &str, build: u64| {
        let mut body = Vec::new();
        enc.encode_device_info(device, version, build, |c| body.extend_from_slice(c));
        enc.encode_ota_request(build, 256, |c| body.extend_from_slice(c));
        client
            .post(format!("{base}/ingest/chunks"))
            .header("Content-Type", "application/octet-stream")
            .body(body)
            .send()
    };
    for d in ["r1", "r2", "r3"] {
        report(d, "1.0.0", 100).await.unwrap();
    }

    let fw: serde_json::Value = client
        .post(format!("{base}/ota/firmware"))
        .header("Authorization", &auth)
        .header("X-Firmware-Version", "2.0.0")
        .header("X-Build-Id", "200")
        .body(vec![1; 32])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let campaign: serde_json::Value = client
        .post(format!("{base}/ota/campaigns"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "v2", "firmware_id": fw["artifact"]["id"], "target_version": "2.0.0"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cid = campaign["campaign"]["id"].as_i64().unwrap();
    client
        .post(format!("{base}/ota/campaigns/{cid}/activate"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    // r1 installs the update.
    report("r1", "2.0.0", 200).await.unwrap();

    let get = |path: String| {
        client
            .get(format!("{base}{path}"))
            .header("Authorization", &auth)
            .send()
    };

    let builds: serde_json::Value = get("/reports/fleet/builds".into())
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(builds["total_devices"], 3);
    let counts: Vec<(u64, i64)> = builds["builds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["build_id"].as_u64().unwrap(),
                b["devices"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(counts, vec![(100, 2), (200, 1)]);

    let reach: serde_json::Value = get(format!("/reports/campaigns/{cid}"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let r = &reach["reach"];
    assert_eq!(r["targeted"], 3);
    assert_eq!(r["on_target"], 1);
    assert_eq!(r["installed"], 1);
    assert_eq!(r["pending"], 2);
    assert!(r["activated_at"].is_string());
    assert_eq!(r["time_to_patch"]["count"], 1);

    let csv = get(format!("/reports/campaigns/{cid}?format=csv"))
        .await
        .unwrap();
    assert!(csv.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/csv"));
    let text = csv.text().await.unwrap();
    let mut lines = text.lines();
    assert!(lines
        .next()
        .unwrap()
        .starts_with("campaign_id,campaign,target_version"));
    assert_eq!(lines.count(), 3);

    let history: serde_json::Value = get("/reports/devices/r1/firmware-history".into())
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let builds: Vec<u64> = history["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["build_id"].as_u64().unwrap())
        .collect();
    assert_eq!(builds, vec![100, 200]);

    // Reports need user auth.
    let anon = Client::new()
        .get(format!("{base}/reports/fleet/builds"))
        .send()
        .await
        .unwrap();
    assert_eq!(anon.status(), StatusCode::UNAUTHORIZED);
}
