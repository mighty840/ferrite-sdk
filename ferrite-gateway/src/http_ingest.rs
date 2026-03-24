//! HTTP ingest listener — accepts chunk POSTs from Ethernet/WiFi devices.
//!
//! Devices on the local network POST raw chunks to the gateway's HTTP endpoint,
//! which then forwards them to the upstream ferrite-server. This is the
//! industrial pattern: all devices route through the edge gateway.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::framing::{ChunkFramer, DecodedChunk};

const MAX_BODY: usize = 8192;

/// Start an HTTP ingest server on the given port.
/// Accepts POST /ingest/chunks with raw chunk bytes in the body.
pub async fn http_ingest_task(port: u16, tx: mpsc::Sender<DecodedChunk>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    info!("HTTP ingest listening on 0.0.0.0:{}", port);

    loop {
        let (mut stream, addr) = listener.accept().await?;
        let tx = tx.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_connection(&mut stream, &tx).await {
                debug!("Connection from {} error: {}", addr, e);
            }
        });
    }
}

async fn handle_connection(
    stream: &mut tokio::net::TcpStream,
    tx: &mpsc::Sender<DecodedChunk>,
) -> anyhow::Result<()> {
    let mut buf = vec![0u8; MAX_BODY + 1024]; // extra for headers
    let mut total = 0;

    // Read the full request (headers + body)
    loop {
        let n = stream.read(&mut buf[total..]).await?;
        if n == 0 {
            break;
        }
        total += n;
        if total >= buf.len() {
            break;
        }
        // Check if we have the full headers
        if let Some(header_end) = find_header_end(&buf[..total]) {
            let content_length = parse_content_length(&buf[..header_end]);
            let body_start = header_end + 4; // skip \r\n\r\n
            let body_needed = content_length.unwrap_or(0);

            if total >= body_start + body_needed {
                break;
            }
        }
    }

    // Parse the request
    let header_end = find_header_end(&buf[..total]).unwrap_or(total);
    let headers = std::str::from_utf8(&buf[..header_end]).unwrap_or("");
    let first_line = headers.lines().next().unwrap_or("");

    let body_start = if header_end + 4 <= total {
        header_end + 4
    } else {
        total
    };
    let body = &buf[body_start..total];

    // Route
    if first_line.starts_with("GET /health") {
        let resp = "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n{\"status\":\"ok\"}";
        stream.write_all(resp.as_bytes()).await?;
    } else if first_line.starts_with("POST /ingest") {
        let mut framer = ChunkFramer::new();
        let chunks = framer.feed(body);
        let count = chunks.len();

        for chunk in chunks {
            debug!(
                "HTTP ingest: chunk type=0x{:02X} len={}",
                chunk.chunk_type, chunk.payload_len
            );
            if tx.send(chunk).await.is_err() {
                error!("Chunk channel closed");
                break;
            }
        }

        if count > 0 {
            debug!(
                "HTTP ingest: received {} chunks ({} bytes)",
                count,
                body.len()
            );
        } else if !body.is_empty() {
            warn!("HTTP ingest: {} bytes but no valid chunks", body.len());
        }

        let resp_body = format!("{{\"ok\":true,\"chunks_received\":{}}}", count);
        let resp = format!(
            "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            resp_body.len(),
            resp_body
        );
        stream.write_all(resp.as_bytes()).await?;
    } else {
        let resp = "HTTP/1.0 404 Not Found\r\n\r\nnot found";
        stream.write_all(resp.as_bytes()).await?;
    }

    Ok(())
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn parse_content_length(headers: &[u8]) -> Option<usize> {
    let s = std::str::from_utf8(headers).ok()?;
    for line in s.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("content-length:") {
            return lower
                .trim_start_matches("content-length:")
                .trim()
                .parse()
                .ok();
        }
    }
    None
}
