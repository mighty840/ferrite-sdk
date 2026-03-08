/// Implement this trait for your transport layer.
/// The SDK calls `send_chunk` for each chunk that needs to be uploaded.
pub trait ChunkTransport {
    type Error: core::fmt::Debug;

    /// Send a single encoded chunk (up to 256 bytes).
    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error>;

    /// Return true if the transport is currently available for sending.
    fn is_available(&self) -> bool {
        true
    }

    /// Called before a batch upload begins. Optional setup.
    fn begin_session(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called after a batch upload completes. Optional teardown.
    fn end_session(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Async variant of ChunkTransport for Embassy.
#[cfg(feature = "embassy")]
pub trait AsyncChunkTransport {
    type Error: core::fmt::Debug;

    async fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error>;

    fn is_available(&self) -> bool {
        true
    }

    async fn begin_session(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn end_session(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Simple blocking UART transport.
/// Sends raw chunk bytes over UART with no framing beyond the chunk format.
pub struct UartTransport<UART> {
    uart: UART,
}

impl<UART> UartTransport<UART> {
    pub fn new(uart: UART) -> Self {
        Self { uart }
    }

    pub fn into_inner(self) -> UART {
        self.uart
    }
}

// Note: embedded-hal UART impl would go here, behind cortex-m feature.
// For now we keep it generic — users implement ChunkTransport for their specific UART.
