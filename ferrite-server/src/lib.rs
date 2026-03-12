pub mod auth;
pub mod auth_middleware;
pub mod config;
pub mod ingest;
pub mod store;
pub mod symbolicate;

use std::path::PathBuf;
use tokio::sync::Mutex;

pub struct AppState {
    pub store: Mutex<store::Store>,
    pub symbolicator: Mutex<symbolicate::Symbolicator>,
    pub elf_dir: PathBuf,
    pub config: &'static config::AuthConfig,
}
