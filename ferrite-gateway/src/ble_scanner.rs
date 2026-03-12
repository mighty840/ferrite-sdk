//! BLE scanner — discovers ferrite devices and receives GATT chunk notifications.

#[cfg(feature = "ble")]
mod inner {
    use anyhow::Result;
    use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter};
    use btleplug::platform::{Manager, Peripheral};
    use futures::StreamExt;
    use tokio::sync::mpsc;
    use tracing::{debug, info, warn};
    use uuid::Uuid;

    use crate::framing::{ChunkFramer, DecodedChunk};

    /// Ferrite BLE service UUID (custom 128-bit).
    pub const FERRITE_SERVICE_UUID: Uuid =
        Uuid::from_u128(0xFE771E00_0001_4000_8000_00805F9B34FB);

    /// Ferrite chunk characteristic UUID (notifications from device).
    pub const CHUNK_CHAR_UUID: Uuid =
        Uuid::from_u128(0xFE771E00_0002_4000_8000_00805F9B34FB);

    /// Scan for ferrite BLE devices and receive chunk notifications.
    pub async fn ble_scanner_task(tx: mpsc::Sender<DecodedChunk>) -> Result<()> {
        let manager = Manager::new().await?;
        let adapters = manager.adapters().await?;
        let adapter = adapters
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No BLE adapter found"))?;

        info!("Starting BLE scan on adapter");

        adapter.start_scan(ScanFilter::default()).await?;

        let mut events = adapter.events().await?;

        while let Some(event) = events.next().await {
            if let CentralEvent::DeviceDiscovered(id) = event {
                let peripheral = match adapter.peripheral(&id).await {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                // Check if this device advertises the ferrite service
                if let Ok(Some(props)) = peripheral.properties().await {
                    if props.services.contains(&FERRITE_SERVICE_UUID) {
                        let name = props.local_name.unwrap_or_else(|| "unknown".into());
                        info!("Found ferrite device: {} ({:?})", name, id);

                        let tx = tx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_ferrite_device(peripheral, tx).await {
                                warn!("BLE device handler error: {}", e);
                            }
                        });
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_ferrite_device(
        peripheral: Peripheral,
        tx: mpsc::Sender<DecodedChunk>,
    ) -> Result<()> {
        peripheral.connect().await?;
        peripheral.discover_services().await?;

        let chars = peripheral.characteristics();
        let chunk_char = chars
            .iter()
            .find(|c| c.uuid == CHUNK_CHAR_UUID)
            .ok_or_else(|| anyhow::anyhow!("Chunk characteristic not found"))?;

        peripheral.subscribe(chunk_char).await?;
        info!("Subscribed to chunk notifications");

        let mut framer = ChunkFramer::new();
        let mut notification_stream = peripheral.notifications().await?;

        while let Some(notification) = notification_stream.next().await {
            if notification.uuid == CHUNK_CHAR_UUID {
                let chunks = framer.feed(&notification.value);
                for chunk in chunks {
                    debug!(
                        "BLE: decoded chunk type=0x{:02X} len={}",
                        chunk.chunk_type, chunk.payload_len
                    );
                    if tx.send(chunk).await.is_err() {
                        warn!("Chunk channel closed, stopping BLE handler");
                        return Ok(());
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(feature = "ble")]
#[allow(unused_imports)]
pub use inner::{ble_scanner_task, CHUNK_CHAR_UUID, FERRITE_SERVICE_UUID};
