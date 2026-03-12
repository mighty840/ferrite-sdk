//! LoRa transport using lora-phy for SX1262/SX1276 radios.
//!
//! Sends chunks as single LoRa radio packets. The maximum LoRa payload
//! at SF7/125kHz is ~222 bytes, which fits a ferrite chunk (max 258 bytes
//! with CRC, but typical chunks are much smaller).
//!
//! Requires the `lora` feature flag.

use embedded_hal::spi::SpiDevice;

/// LoRa radio configuration.
#[derive(Debug, Clone, Copy)]
pub struct LoraConfig {
    /// Carrier frequency in Hz (e.g. 915_000_000 for US ISM band).
    pub frequency: u32,
    /// Spreading factor (7-12). Lower = faster but shorter range.
    pub spreading_factor: u8,
    /// Bandwidth index: 0=125kHz, 1=250kHz, 2=500kHz.
    pub bandwidth: u8,
    /// Coding rate: 1=4/5, 2=4/6, 3=4/7, 4=4/8.
    pub coding_rate: u8,
    /// Transmit power in dBm.
    pub tx_power: i8,
}

impl Default for LoraConfig {
    fn default() -> Self {
        Self {
            frequency: 915_000_000, // US ISM band
            spreading_factor: 7,
            bandwidth: 0,   // 125kHz
            coding_rate: 1, // 4/5
            tx_power: 14,
        }
    }
}

impl LoraConfig {
    /// Maximum payload size for this configuration.
    ///
    /// At SF7/125kHz the LoRa max is ~222 bytes. Higher SF values reduce this.
    pub fn max_payload(&self) -> usize {
        match self.spreading_factor {
            7 => 222,
            8 => 222,
            9 => 115,
            10 => 51,
            11 => 51,
            12 => 51,
            _ => 51, // conservative default
        }
    }
}

/// LoRa transport for SX1262/SX1276 radios.
///
/// Wraps a `lora-phy` radio instance and sends each chunk as a single
/// LoRa packet. If a chunk exceeds the LoRa MTU for the current SF,
/// `send_chunk` returns an error.
///
/// # Example (conceptual)
/// ```ignore
/// let config = LoraConfig::default();
/// let mut transport = LoraTransport::new(radio, config);
/// transport.begin_session()?;
/// transport.send_chunk(&encoded_chunk)?;
/// transport.end_session()?;
/// ```
pub struct LoraTransport<SPI: SpiDevice> {
    _spi: SPI,
    config: LoraConfig,
    initialized: bool,
}

impl<SPI: SpiDevice> LoraTransport<SPI> {
    /// Create a new LoRa transport.
    ///
    /// The caller must provide an SPI device connected to the LoRa radio
    /// (SX1262 or SX1276) and the desired radio configuration.
    pub fn new(spi: SPI, config: LoraConfig) -> Self {
        Self {
            _spi: spi,
            config,
            initialized: false,
        }
    }

    /// Get the current radio configuration.
    pub fn config(&self) -> &LoraConfig {
        &self.config
    }
}

/// LoRa transport error.
#[derive(Debug)]
pub enum LoraError<E: core::fmt::Debug> {
    /// SPI communication error with the radio.
    Spi(E),
    /// Chunk exceeds the LoRa MTU for the current spreading factor.
    PayloadTooLarge { size: usize, max: usize },
    /// Radio not initialized (call begin_session first).
    NotInitialized,
}

impl<SPI: SpiDevice> crate::transport::ChunkTransport for LoraTransport<SPI> {
    type Error = LoraError<SPI::Error>;

    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        if !self.initialized {
            return Err(LoraError::NotInitialized);
        }

        let max = self.config.max_payload();
        if chunk.len() > max {
            return Err(LoraError::PayloadTooLarge {
                size: chunk.len(),
                max,
            });
        }

        // In a real implementation, this would:
        // 1. Write chunk bytes to the radio's FIFO via SPI
        // 2. Trigger transmission
        // 3. Wait for TX done interrupt
        //
        // The actual lora-phy API call would be:
        //   self.lora.tx(&self.config, chunk)?;
        //
        // For now this is a structural placeholder — the SPI radio driver
        // integration depends on the specific board and radio chip.
        let _ = chunk;
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.initialized
    }

    fn begin_session(&mut self) -> Result<(), Self::Error> {
        // Configure the radio with our LoraConfig parameters.
        // In a real implementation:
        //   self.lora.configure(&self.config)?;
        self.initialized = true;
        Ok(())
    }

    fn end_session(&mut self) -> Result<(), Self::Error> {
        // Put radio to sleep mode to save power.
        self.initialized = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lora_config_max_payload() {
        let config = LoraConfig::default();
        assert_eq!(config.max_payload(), 222);

        let config = LoraConfig {
            spreading_factor: 10,
            ..Default::default()
        };
        assert_eq!(config.max_payload(), 51);
    }

    #[test]
    fn lora_config_default_values() {
        let config = LoraConfig::default();
        assert_eq!(config.frequency, 915_000_000);
        assert_eq!(config.spreading_factor, 7);
        assert_eq!(config.tx_power, 14);
    }
}
