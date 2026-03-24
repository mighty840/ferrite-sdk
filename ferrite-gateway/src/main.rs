//! ferrite-gateway — edge gateway daemon that bridges BLE/USB/LoRa devices
//! to a ferrite-server instance over HTTP.

mod ble_scanner;
mod buffer;
mod config;
mod forwarder;
mod framing;
mod http_ingest;
mod usb_reader;

use anyhow::Result;
use clap::Parser;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use buffer::ChunkBuffer;
use config::GatewayConfig;
use forwarder::Forwarder;
use framing::DecodedChunk;

#[derive(Parser)]
#[command(
    name = "ferrite-gateway",
    about = "Edge gateway for ferrite IoT devices"
)]
struct Cli {
    /// Server URL (overrides FERRITE_SERVER_URL env var)
    #[arg(long)]
    server: Option<String>,

    /// USB serial port path(s) — can be specified multiple times
    #[cfg(feature = "usb")]
    #[arg(long = "usb-port")]
    usb_ports: Vec<String>,

    /// USB baud rate
    #[cfg(feature = "usb")]
    #[arg(long, default_value = "115200")]
    usb_baud: u32,

    /// Enable BLE scanning
    #[cfg(feature = "ble")]
    #[arg(long)]
    ble: bool,

    /// HTTP ingest port for Ethernet/WiFi devices (0 = disabled)
    #[arg(long, default_value = "4001")]
    http_ingest_port: u16,

    /// Buffer database path
    #[arg(long, default_value = "ferrite-gateway.db")]
    buffer_db: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let mut config = GatewayConfig::from_env();

    // CLI overrides
    if let Some(server) = cli.server {
        config.server_url = server;
    }
    #[cfg(feature = "usb")]
    {
        if !cli.usb_ports.is_empty() {
            config.usb_ports = cli.usb_ports;
        }
        config.usb_baud = cli.usb_baud;
    }
    config.buffer_db = cli.buffer_db;

    info!("ferrite-gateway starting");
    info!("Server: {}", config.server_url);

    let forwarder = Forwarder::new(&config);

    // Check server health
    if forwarder.health_check().await {
        info!("Server is reachable");
    } else {
        warn!("Server is not reachable — chunks will be buffered");
    }

    let buffer = ChunkBuffer::open(&config.buffer_db)?;
    let buffered_count = buffer.count()?;
    if buffered_count > 0 {
        info!("{} chunks buffered from previous session", buffered_count);
    }

    // Channel for all receivers to send decoded chunks
    let (chunk_tx, mut chunk_rx) = mpsc::channel::<DecodedChunk>(256);

    // Start USB readers — one task per configured port
    #[cfg(feature = "usb")]
    for port in &config.usb_ports {
        let port = port.clone();
        let baud = config.usb_baud;
        let tx = chunk_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = usb_reader::usb_reader_task(port.clone(), baud, tx).await {
                error!("USB reader for {} failed: {}", port, e);
            }
        });
    }

    // Start BLE scanner if enabled
    #[cfg(feature = "ble")]
    if cli.ble {
        let tx = chunk_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = ble_scanner::ble_scanner_task(tx).await {
                error!("BLE scanner failed: {}", e);
            }
        });
    }

    // Start HTTP ingest listener for Ethernet/WiFi devices
    if cli.http_ingest_port > 0 {
        let tx = chunk_tx.clone();
        let port = cli.http_ingest_port;
        tokio::spawn(async move {
            if let Err(e) = http_ingest::http_ingest_task(port, tx).await {
                error!("HTTP ingest listener failed: {}", e);
            }
        });
    }

    // Drop our copy of the sender so the channel closes when all receivers stop
    drop(chunk_tx);

    // Main forwarding loop
    info!("Gateway ready — waiting for chunks");

    // First, drain any buffered chunks
    drain_buffer(&buffer, &forwarder).await;

    while let Some(chunk) = chunk_rx.recv().await {
        // Batch chunks that arrive within a short window (same upload session).
        // The server parses multiple concatenated chunks from one POST body,
        // and uses the DeviceInfo chunk to associate all chunks with the right device.
        let mut batch = chunk.raw.clone();
        let batch_start = tokio::time::Instant::now();
        loop {
            match tokio::time::timeout(
                std::time::Duration::from_millis(200),
                chunk_rx.recv(),
            )
            .await
            {
                Ok(Some(next)) => {
                    batch.extend_from_slice(&next.raw);
                    // Cap batch time to avoid holding chunks too long
                    if batch_start.elapsed() > std::time::Duration::from_secs(2) {
                        break;
                    }
                }
                _ => break, // Timeout or channel closed
            }
        }

        debug!("Forwarding batch of {} bytes", batch.len());
        match forwarder.forward_chunk(&batch).await {
            Ok(true) => {
                // Successfully forwarded
            }
            Ok(false) => {
                warn!("Server rejected batch");
            }
            Err(_) => {
                info!("Buffering batch (server unreachable)");
                if let Err(e) = buffer.enqueue(None, &batch) {
                    error!("Failed to buffer batch: {}", e);
                }
            }
        }
    }

    info!("All receivers stopped, gateway shutting down");
    Ok(())
}

/// Attempt to forward all buffered chunks to the server.
async fn drain_buffer(buffer: &ChunkBuffer, forwarder: &Forwarder) {
    loop {
        let items = match buffer.peek(50) {
            Ok(items) if items.is_empty() => break,
            Ok(items) => items,
            Err(e) => {
                error!("Failed to read buffer: {}", e);
                break;
            }
        };

        for (id, data) in &items {
            match forwarder.forward_chunk(data).await {
                Ok(_) => {
                    let _ = buffer.remove(*id);
                }
                Err(_) => {
                    warn!("Server still unreachable, stopping buffer drain");
                    return;
                }
            }
        }
    }
}
