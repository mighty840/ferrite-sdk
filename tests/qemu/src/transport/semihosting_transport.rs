use cortex_m_semihosting::hio;
use ferrite_sdk::transport::ChunkTransport;

pub struct SemihostingTransport {
    pub chunks_sent: u32,
}

impl SemihostingTransport {
    pub fn new() -> Self {
        Self { chunks_sent: 0 }
    }
}

impl ChunkTransport for SemihostingTransport {
    type Error = ();

    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), ()> {
        if let Ok(mut stdout) = hio::hstdout() {
            let _ = stdout.write_all(chunk);
        }
        self.chunks_sent += 1;
        Ok(())
    }
}
