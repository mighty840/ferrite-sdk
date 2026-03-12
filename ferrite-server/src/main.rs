mod report;

use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use ferrite_server::config::AuthConfig;
use ferrite_server::prometheus::RequestCounters;
use ferrite_server::rate_limit::RateLimiter;
use ferrite_server::store::Store;
use ferrite_server::symbolicate::Symbolicator;
use ferrite_server::AppState;

#[derive(Parser)]
#[command(
    name = "ferrite-server",
    about = "Companion ingestion server for ferrite-sdk"
)]
struct Cli {
    /// HTTP listen address
    #[arg(long, default_value = "0.0.0.0:4000")]
    http: SocketAddr,

    /// SQLite database path
    #[arg(long, default_value = "./ferrite.db")]
    db: PathBuf,

    /// Directory for uploaded ELF files
    #[arg(long, default_value = "./elfs")]
    elf_dir: PathBuf,

    /// Path to arm-none-eabi-addr2line (auto-detect if omitted)
    #[arg(long)]
    addr2line: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the HTTP server (default)
    Serve,
    /// Print a summary report of all devices
    Report,
    /// List recent fault events
    Faults,
    /// List recent metrics
    Metrics,
    /// Export a database backup
    Backup {
        /// Output file path
        #[arg(long, default_value = "./ferrite-backup.db")]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Load .env file if present
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    // Ensure elf directory exists
    std::fs::create_dir_all(&cli.elf_dir)?;

    // Load auth config from environment (leaked for 'static lifetime)
    let config: &'static AuthConfig = Box::leak(Box::new(AuthConfig::from_env()));

    let store = Store::open(&cli.db)?;
    let symbolicator = Symbolicator::new(cli.addr2line.clone(), cli.elf_dir.clone());

    // SSE broadcast channel (capacity 256 — lagged receivers drop old events)
    let (event_tx, _) = tokio::sync::broadcast::channel(256);

    // Rate limiter (if configured)
    let rate_limiter = config.rate_limit_rps.and_then(|rps| {
        if rps > 0.0 {
            let burst = (rps * 10.0).max(10.0); // 10s burst window
            tracing::info!("Rate limiting: {rps} req/s per IP (burst: {burst})");
            Some(Arc::new(RateLimiter::new(rps, burst)))
        } else {
            None
        }
    });

    let state = Arc::new(AppState {
        store: Mutex::new(store),
        symbolicator: Mutex::new(symbolicator),
        elf_dir: cli.elf_dir.clone(),
        config,
        event_tx,
        counters: RequestCounters::new(),
        rate_limiter: rate_limiter.clone(),
    });

    if config.ingest_api_key.is_none() {
        tracing::warn!("INGEST_API_KEY is not set — ingest endpoints are publicly accessible");
    }

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => {
            // Start background retention purge task (#29)
            ferrite_server::retention::spawn_retention_task(state.clone());

            // Start alerting offline-check task (#28)
            ferrite_server::alerting::spawn_offline_check_task(state.clone());

            // Start rate limiter cleanup task (#34)
            if let Some(ref limiter) = rate_limiter {
                ferrite_server::rate_limit::spawn_cleanup_task(limiter.clone());
            }

            tracing::info!("Starting ferrite-server on {}", cli.http);
            let app = ferrite_server::ingest::router(state);
            let listener = tokio::net::TcpListener::bind(cli.http).await?;
            axum::serve(listener, app).await?;
        }
        Command::Report => {
            let st = state.store.lock().await;
            report::print_report(&st)?;
        }
        Command::Faults => {
            let st = state.store.lock().await;
            report::print_faults(&st)?;
        }
        Command::Metrics => {
            let st = state.store.lock().await;
            report::print_metrics(&st)?;
        }
        Command::Backup { output } => {
            tracing::info!("Creating database backup at {}", output.display());
            let st = state.store.lock().await;
            let bytes = st.backup_to_bytes()?;
            std::fs::write(&output, bytes)?;
            tracing::info!("Backup complete: {}", output.display());
        }
    }

    Ok(())
}
