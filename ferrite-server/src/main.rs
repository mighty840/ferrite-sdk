mod report;

use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use ferrite_server::config::AuthConfig;
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

    let state = Arc::new(AppState {
        store: Mutex::new(store),
        symbolicator: Mutex::new(symbolicator),
        elf_dir: cli.elf_dir.clone(),
        config,
    });

    if config.ingest_api_key.is_none() {
        tracing::warn!("INGEST_API_KEY is not set — ingest endpoints are publicly accessible");
    }

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => {
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
    }

    Ok(())
}
