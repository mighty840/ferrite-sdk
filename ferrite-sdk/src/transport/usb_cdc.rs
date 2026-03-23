//! USB CDC ACM transport for embassy-usb.
//!
//! Sends chunks over a USB CDC ACM (virtual serial) connection.
//! Works with any board that has USB OTG: STM32, nRF52840, RP2040.
//!
//! Requires the `usb-cdc` feature flag.

use embassy_usb::class::cdc_acm::CdcAcmClass;
use embassy_usb::driver::{Driver, EndpointError};

/// USB CDC transport that sends chunks as raw bytes over the virtual serial port.
///
/// The host side (gateway or PC) receives bytes and uses [`ChunkFramer`] to
/// extract complete chunks by syncing on the 0xEC magic byte.
pub struct UsbCdcTransport<'d, D: Driver<'d>> {
    class: CdcAcmClass<'d, D>,
}

impl<'d, D: Driver<'d>> UsbCdcTransport<'d, D> {
    /// Create a new USB CDC transport from an already-configured CDC ACM class.
    ///
    /// The caller is responsible for creating the `CdcAcmClass` from the USB
    /// device builder and running the USB device task concurrently.
    pub fn new(class: CdcAcmClass<'d, D>) -> Self {
        Self { class }
    }

    /// Check if the host has the DTR (Data Terminal Ready) signal set,
    /// indicating a terminal or gateway is listening.
    pub fn dtr(&self) -> bool {
        self.class.dtr()
    }
}

impl<'d, D: Driver<'d>> crate::transport::AsyncChunkTransport for UsbCdcTransport<'d, D> {
    type Error = EndpointError;

    async fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        // Wait for DTR before sending
        self.class.wait_connection().await;

        // Write the entire chunk as a single USB transfer.
        // The max USB CDC packet size is typically 64 bytes, so embassy-usb
        // handles fragmentation internally.
        self.class.write_packet(chunk).await?;
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.class.dtr()
    }

    async fn begin_session(&mut self) -> Result<(), Self::Error> {
        self.class.wait_connection().await;
        Ok(())
    }

    async fn end_session(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
