//! BLE scanner — discovers ferrite devices and receives GATT chunk notifications.

/// Whether advertisement data identifies a ferrite device: the ferrite
/// service UUID, or a name containing "ferrite" or "nrf5340-zephyr"
/// (BlueZ on older distros may not populate service UUIDs from advertisements).
#[cfg_attr(not(feature = "ble"), allow(dead_code))]
fn is_ferrite_advertisement(local_name: &str, advertises_ferrite_service: bool) -> bool {
    advertises_ferrite_service
        || local_name.to_lowercase().contains("ferrite")
        || local_name.contains("nrf5340-zephyr")
}

#[cfg(feature = "ble")]
mod inner {
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use anyhow::Result;
    use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter};
    use btleplug::platform::{Adapter, Manager, Peripheral, PeripheralId};
    use futures::StreamExt;
    use tokio::sync::mpsc;
    use tracing::{debug, info, warn};
    use uuid::Uuid;

    use super::is_ferrite_advertisement;
    use crate::framing::{ChunkFramer, DecodedChunk};

    /// Ferrite BLE service UUID (custom 128-bit).
    pub const FERRITE_SERVICE_UUID: Uuid = Uuid::from_u128(0xFE771E00_0001_4000_8000_00805F9B34FB);

    /// Ferrite chunk characteristic UUID (notifications from device).
    pub const CHUNK_CHAR_UUID: Uuid = Uuid::from_u128(0xFE771E00_0002_4000_8000_00805F9B34FB);

    /// How often known peripherals are re-checked, so a device that dropped
    /// off is reconnected even if BlueZ never emits a new discovery event.
    const RESCAN_INTERVAL: Duration = Duration::from_secs(30);

    /// Peripherals that currently have a handler, so discovery events and the
    /// periodic re-check never connect the same device twice.
    type ActiveSet = Arc<Mutex<HashSet<PeripheralId>>>;

    /// Scan for ferrite BLE devices and receive chunk notifications.
    ///
    /// A connection that drops, or whose notifications go quiet for
    /// `idle_timeout`, is torn down and the device is picked up again on the
    /// next discovery event or periodic re-check.
    pub async fn ble_scanner_task(
        tx: mpsc::Sender<DecodedChunk>,
        idle_timeout: Duration,
    ) -> Result<()> {
        let manager = Manager::new().await?;
        let adapters = manager.adapters().await?;
        let adapter = adapters
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No BLE adapter found"))?;

        info!("Starting BLE scan on adapter");

        adapter.start_scan(ScanFilter::default()).await?;

        let mut events = adapter.events().await?;
        let active: ActiveSet = Arc::new(Mutex::new(HashSet::new()));
        let mut rescan = tokio::time::interval(RESCAN_INTERVAL);

        loop {
            tokio::select! {
                event = events.next() => {
                    let Some(event) = event else { break };
                    // Not DeviceUpdated: it fires on every RSSI change of every
                    // nearby device; the periodic re-check covers reconnects.
                    match event {
                        CentralEvent::DeviceDiscovered(id) => {
                            consider(&adapter, id, &active, &tx, idle_timeout).await;
                        }
                        CentralEvent::DeviceDisconnected(id) => {
                            debug!("BLE device disconnected: {:?}", id);
                        }
                        _ => {}
                    }
                }
                _ = rescan.tick() => {
                    if tx.is_closed() {
                        break;
                    }
                    // BlueZ can stop discovery on its own; restarting an
                    // active scan is harmless.
                    if let Err(e) = adapter.start_scan(ScanFilter::default()).await {
                        debug!("BLE start_scan: {}", e);
                    }
                    for peripheral in adapter.peripherals().await.unwrap_or_default() {
                        consider(&adapter, peripheral.id(), &active, &tx, idle_timeout).await;
                    }
                }
            }
        }

        Ok(())
    }

    /// Spawn a handler for `id` if it is a ferrite device without one.
    async fn consider(
        adapter: &Adapter,
        id: PeripheralId,
        active: &ActiveSet,
        tx: &mpsc::Sender<DecodedChunk>,
        idle_timeout: Duration,
    ) {
        if active.lock().unwrap().contains(&id) {
            return;
        }
        let peripheral = match adapter.peripheral(&id).await {
            Ok(p) => p,
            Err(_) => return,
        };
        let Ok(Some(props)) = peripheral.properties().await else {
            return;
        };
        let name = props.local_name.clone().unwrap_or_default();
        if !is_ferrite_advertisement(&name, props.services.contains(&FERRITE_SERVICE_UUID)) {
            return;
        }
        // Re-check under the lock: another event may have claimed it meanwhile.
        if !active.lock().unwrap().insert(id.clone()) {
            return;
        }

        let name = if name.is_empty() {
            "unknown".to_string()
        } else {
            name
        };
        info!("Found ferrite device: {} ({:?})", name, id);

        let tx = tx.clone();
        let active = active.clone();
        tokio::spawn(async move {
            match handle_ferrite_device(&peripheral, tx, idle_timeout).await {
                Ok(()) => info!("BLE device {} disconnected, will reconnect", name),
                Err(e) => warn!("BLE device handler error ({}): {}", name, e),
            }
            let _ = peripheral.disconnect().await;
            active.lock().unwrap().remove(&id);
        });
    }

    async fn handle_ferrite_device(
        peripheral: &Peripheral,
        tx: mpsc::Sender<DecodedChunk>,
        idle_timeout: Duration,
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

        loop {
            // A stale subscription (link dropped without BlueZ ending the
            // stream) shows up as silence; treat it as a disconnect.
            let notification =
                match tokio::time::timeout(idle_timeout, notification_stream.next()).await {
                    Ok(Some(n)) => n,
                    Ok(None) => return Ok(()),
                    Err(_) => {
                        warn!(
                            "No BLE notifications for {}s, reconnecting",
                            idle_timeout.as_secs()
                        );
                        return Ok(());
                    }
                };

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
    }
}

#[cfg(feature = "ble")]
#[allow(unused_imports)]
pub use inner::{ble_scanner_task, CHUNK_CHAR_UUID, FERRITE_SERVICE_UUID};

#[cfg(test)]
mod tests {
    use super::is_ferrite_advertisement;

    #[test]
    fn matches_service_uuid_or_known_names() {
        assert!(is_ferrite_advertisement("", true));
        assert!(is_ferrite_advertisement("Ferrite nRF5340", false));
        assert!(is_ferrite_advertisement("nrf5340-zephyr-01", false));
        assert!(!is_ferrite_advertisement("JBL Flip 5", false));
        assert!(!is_ferrite_advertisement("", false));
    }
}
