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
