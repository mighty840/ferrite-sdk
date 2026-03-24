//! Gateway configuration loaded from environment or CLI args.

use serde::{Deserialize, Serialize};

/// Top-level gateway configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Ferrite server base URL (e.g. "http://localhost:4000").
    pub server_url: String,

    /// Basic auth username (if server uses basic auth).
    pub auth_user: Option<String>,

    /// Basic auth password.
    pub auth_pass: Option<String>,

    /// Ingest API key (sent as X-API-Key header).
    pub api_key: Option<String>,

    /// Path to SQLite buffer database for offline queueing.
    pub buffer_db: String,

    /// USB serial port paths (e.g. "/dev/ttyACM0", "/dev/ttyACM1").
    #[cfg(feature = "usb")]
    pub usb_ports: Vec<String>,

    /// USB baud rate.
    #[cfg(feature = "usb")]
    pub usb_baud: u32,

    /// Maximum retry attempts for HTTP forwarding.
    pub max_retries: u32,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            server_url: "http://localhost:4000".to_string(),
            auth_user: None,
            auth_pass: None,
            api_key: None,
            buffer_db: "ferrite-gateway.db".to_string(),
            #[cfg(feature = "usb")]
            usb_ports: Vec::new(),
            #[cfg(feature = "usb")]
            usb_baud: 115200,
            max_retries: 3,
        }
    }
}

impl GatewayConfig {
    /// Load config from environment variables (with dotenvy .env support).
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();
        let mut cfg = Self::default();

        if let Ok(v) = std::env::var("FERRITE_SERVER_URL") {
            cfg.server_url = v;
        }
        if let Ok(v) = std::env::var("BASIC_AUTH_USER") {
            cfg.auth_user = Some(v);
        }
        if let Ok(v) = std::env::var("BASIC_AUTH_PASS") {
            cfg.auth_pass = Some(v);
        }
        if let Ok(v) = std::env::var("INGEST_API_KEY") {
            cfg.api_key = Some(v);
        }
        if let Ok(v) = std::env::var("GATEWAY_BUFFER_DB") {
            cfg.buffer_db = v;
        }
        #[cfg(feature = "usb")]
        {
            if let Ok(v) = std::env::var("USB_PORTS") {
                cfg.usb_ports = v.split(',').map(|s| s.trim().to_string()).collect();
            } else if let Ok(v) = std::env::var("USB_PORT") {
                cfg.usb_ports = vec![v];
            }
            if let Ok(v) = std::env::var("USB_BAUD") {
                if let Ok(baud) = v.parse() {
                    cfg.usb_baud = baud;
                }
            }
        }
        if let Ok(v) = std::env::var("GATEWAY_MAX_RETRIES") {
            if let Ok(n) = v.parse() {
                cfg.max_retries = n;
            }
        }

        cfg
    }
}
