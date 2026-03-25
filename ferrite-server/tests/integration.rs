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
