//! USB CDC serial reader — reads chunks from USB-connected ferrite devices.

#[cfg(feature = "usb")]
mod inner {
    use anyhow::{Context, Result};
    use serialport::SerialPort;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use tracing::{debug, error, info, warn};

    use crate::framing::{ChunkFramer, DecodedChunk};

    /// Read chunks from a USB CDC serial port and send them through a channel.
    pub async fn usb_reader_task(
        port_name: String,
        baud_rate: u32,
        tx: mpsc::Sender<DecodedChunk>,
    ) -> Result<()> {
        info!(
            "Opening USB serial port {} at {} baud",
            port_name, baud_rate
        );

        let port = serialport::new(&port_name, baud_rate)
            .timeout(Duration::from_millis(100))
            .open()
            .with_context(|| format!("Failed to open serial port {port_name}"))?;

        // Run blocking serial reads in a dedicated thread
        let (byte_tx, mut byte_rx) = mpsc::channel::<Vec<u8>>(64);

        std::thread::spawn(move || {
            serial_read_loop(port, byte_tx);
        });

        let mut framer = ChunkFramer::new();

        while let Some(data) = byte_rx.recv().await {
            let chunks = framer.feed(&data);
            for chunk in chunks {
                debug!(
                    "USB: decoded chunk type=0x{:02X} len={}",
                    chunk.chunk_type, chunk.payload_len
                );
                if tx.send(chunk).await.is_err() {
                    warn!("Chunk channel closed, stopping USB reader");
                    return Ok(());
                }
            }
        }

        info!("USB serial port closed");
        Ok(())
    }

    fn serial_read_loop(mut port: Box<dyn SerialPort>, tx: mpsc::Sender<Vec<u8>>) {
        let mut buf = [0u8; 512];
        loop {
            match port.read(&mut buf) {
                Ok(n) if n > 0 => {
                    if tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Ok(_) => {} // zero bytes, timeout
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    // Normal timeout, continue
                }
                Err(e) => {
                    error!("USB serial read error: {}", e);
                    break;
                }
            }
        }
    }
}

#[cfg(feature = "usb")]
pub use inner::usb_reader_task;
