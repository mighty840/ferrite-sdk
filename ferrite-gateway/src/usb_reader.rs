//! USB CDC serial reader — reads chunks from USB-connected ferrite devices.

#[cfg(feature = "usb")]
mod inner {
    use anyhow::{Context, Result};
    use serialport::SerialPort;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use tracing::{debug, error, info, warn};

    use crate::backoff::Backoff;
    use crate::framing::{ChunkFramer, DecodedChunk};

    /// Read chunks from a USB CDC serial port and send them through a channel.
    ///
    /// Runs until the chunk channel closes. When the port disappears (board
    /// reset or replug → `Broken pipe`) or can't be opened, it waits with
    /// exponential backoff and reopens the same path, so a udev symlink that
    /// now points at a new ttyACM node is picked up.
    pub async fn usb_reader_task(
        port_name: String,
        baud_rate: u32,
        tx: mpsc::Sender<DecodedChunk>,
    ) -> Result<()> {
        let mut backoff = Backoff::default();
        let mut reconnecting = false;

        loop {
            match open_port(&port_name, baud_rate) {
                Ok(port) => {
                    if reconnecting {
                        info!("USB serial port {} reconnected", port_name);
                    }
                    backoff.reset();
                    if read_until_closed(port, &tx).await == ReadEnd::ChannelClosed {
                        warn!("Chunk channel closed, stopping USB reader");
                        return Ok(());
                    }
                    info!("USB serial port {} closed, will reopen", port_name);
                    reconnecting = true;
                    // Give udev a moment to remove the old node. Not counted
                    // as a failure, so a failed reopen is still logged at WARN.
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                Err(e) => {
                    // First failure at WARN, repeats at DEBUG so an unplugged
                    // board doesn't flood the journal.
                    if backoff.failures() == 0 {
                        warn!("{:#}", e);
                    } else {
                        debug!("{:#}", e);
                    }
                }
            }

            if tx.is_closed() {
                return Ok(());
            }
            tokio::time::sleep(backoff.next_delay()).await;
        }
    }

    fn open_port(port_name: &str, baud_rate: u32) -> Result<Box<dyn SerialPort>> {
        debug!(
            "Opening USB serial port {} at {} baud",
            port_name, baud_rate
        );

        let mut port = serialport::new(port_name, baud_rate)
            .timeout(Duration::from_millis(100))
            .open()
            .with_context(|| format!("Failed to open serial port {port_name}"))?;
        info!("Opened USB serial port {} at {} baud", port_name, baud_rate);

        // Set DTR (Data Terminal Ready) so USB CDC devices know a host is listening.
        if let Err(e) = port.write_data_terminal_ready(true) {
            warn!("Failed to set DTR on {}: {} (non-fatal)", port_name, e);
        }

        Ok(port)
    }

    #[derive(PartialEq)]
    enum ReadEnd {
        PortClosed,
        ChannelClosed,
    }

    async fn read_until_closed(
        port: Box<dyn SerialPort>,
        tx: &mpsc::Sender<DecodedChunk>,
    ) -> ReadEnd {
        // Run blocking serial reads in a dedicated thread
        let (byte_tx, mut byte_rx) = mpsc::channel::<Vec<u8>>(64);

        std::thread::spawn(move || {
            serial_read_loop(port, byte_tx);
        });

        // A fresh framer per connection drops any partial frame from before
        // the disconnect.
        let mut framer = ChunkFramer::new();

        while let Some(data) = byte_rx.recv().await {
            let chunks = framer.feed(&data);
            for chunk in chunks {
                debug!(
                    "USB: decoded chunk type=0x{:02X} len={}",
                    chunk.chunk_type, chunk.payload_len
                );
                if tx.send(chunk).await.is_err() {
                    return ReadEnd::ChannelClosed;
                }
            }
        }

        ReadEnd::PortClosed
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
                Ok(_) => {
                    // A read that polled ready but returned no bytes is EOF:
                    // the device hung up. Idle periods surface as TimedOut.
                    info!("USB serial port hung up");
                    break;
                }
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
