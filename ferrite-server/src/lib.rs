pub mod alerting;
pub mod auth;
pub mod auth_middleware;
pub mod backup;
pub mod campaigns;
pub mod config;
pub mod crashes;
pub mod groups;
pub mod ingest;
pub mod ota;
pub mod prometheus;
pub mod rate_limit;
pub mod retention;
pub mod sse;
pub mod store;
pub mod symbolicate;

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

pub struct AppState {
    pub store: Mutex<store::Store>,
    pub symbolicator: Mutex<symbolicate::Symbolicator>,
    pub elf_dir: PathBuf,
    pub firmware_dir: PathBuf,
    pub config: &'static config::AuthConfig,
    pub event_tx: broadcast::Sender<sse::SsePayload>,
    pub counters: prometheus::RequestCounters,
    pub rate_limiter: Option<Arc<rate_limit::RateLimiter>>,
}
