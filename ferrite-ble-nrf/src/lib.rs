//! BLE transport for ferrite-sdk on nRF52840.
//!
//! This crate provides a GATT server with a custom ferrite service for
//! streaming chunks over BLE notifications. A companion BLE scanner
//! (in `ferrite-gateway`) connects to the device and forwards chunks
//! to the ferrite-server.
//!
//! # Architecture
//!
//! ```text
//! [nRF52840 firmware]
//!   └─ BleTransport (GATT server, notifies chunks)
//!         │
//!         ├─ Service UUID: FE771E00-0001-4000-8000-00805F9B34FB
//!         └─ Chunk Characteristic: FE771E00-0002-4000-8000-00805F9B34FB
//!               │
//!               ▼
//! [ferrite-gateway BLE scanner]
//!   └─ Subscribes to notifications, forwards to server
//! ```
//!
//! # Usage
//!
//! This crate requires `nrf-softdevice` which depends on the Nordic SoftDevice
//! BLE stack. It can only be built for `thumbv7em-none-eabihf` targets.
//!
//! ```ignore
//! // In your nRF52840 firmware:
//! use ferrite_ble_nrf::BleTransport;
//!
//! let transport = BleTransport::new(softdevice_server, chunk_characteristic);
//! ferrite_embassy::upload_task(&mut transport).await;
//! ```

#![no_std]

/// Ferrite BLE GATT service UUID (128-bit).
///
/// Used by both the device (advertises this UUID) and the gateway
/// (scans for this UUID to identify ferrite devices).
pub const FERRITE_SERVICE_UUID: u128 = 0xFE771E00_0001_4000_8000_00805F9B34FB;

/// Ferrite chunk characteristic UUID (128-bit).
///
/// The device writes chunk data to this characteristic via notifications.
/// The gateway subscribes to receive the chunk bytes.
pub const CHUNK_CHAR_UUID: u128 = 0xFE771E00_0002_4000_8000_00805F9B34FB;

/// Maximum BLE notification payload size after ATT overhead.
///
/// With a negotiated MTU of 247 (common for BLE 5.0), the usable
/// notification payload is 244 bytes — enough for most ferrite chunks.
pub const MAX_BLE_PAYLOAD: usize = 244;

// The actual BleTransport implementation requires nrf-softdevice which
// can only compile for nRF targets. The struct and trait impl are defined
// here as documentation/reference for firmware developers.
//
// A full implementation would look like:
//
// ```rust
// pub struct BleTransport<'a> {
//     server: &'a nrf_softdevice::ble::gatt_server::Server,
//     chunk_handle: u16,
//     conn: Option<nrf_softdevice::ble::Connection>,
// }
//
// impl AsyncChunkTransport for BleTransport<'_> {
//     type Error = BleError;
//
//     async fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
//         let conn = self.conn.as_ref().ok_or(BleError::NotConnected)?;
//         nrf_softdevice::ble::gatt_server::notify_value(
//             conn, self.chunk_handle, chunk
//         ).map_err(BleError::Softdevice)
//     }
// }
// ```
