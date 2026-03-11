use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::time::Duration;

/// UART command bytes for the provisioning protocol.
const CMD_PING: u8 = 0x50;
const CMD_PROVISION: u8 = 0x52;
const CMD_READ_KEY: u8 = 0x4B;
const CMD_CLEAR_KEY: u8 = 0x43;

/// Response bytes.
const RESP_ACK: u8 = 0x06;
const RESP_NAK: u8 = 0x15;

pub struct UartConnection {
    port: Box<dyn serialport::SerialPort>,
}

impl UartConnection {
    pub fn open(port_name: &str, baud: u32) -> Result<Self> {
        let port = serialport::new(port_name, baud)
            .timeout(Duration::from_secs(3))
            .open()
            .with_context(|| format!("failed to open {port_name}"))?;
        Ok(Self { port })
    }

    /// Send a PING command and check for ACK.
    pub fn ping(&mut self) -> Result<bool> {
        self.port.write_all(&[CMD_PING])?;
        self.port.flush()?;
        let mut buf = [0u8; 1];
        match self.port.read_exact(&mut buf) {
            Ok(()) => Ok(buf[0] == RESP_ACK),
            Err(_) => Ok(false),
        }
    }

    /// Send a PROVISION command with owner_prefix and entropy_seed.
    /// Returns the 4-byte device key.
    pub fn provision(&mut self, owner_prefix: u8, entropy_seed: u32) -> Result<u32> {
        let mut cmd = [0u8; 6];
        cmd[0] = CMD_PROVISION;
        cmd[1] = owner_prefix;
        cmd[2..6].copy_from_slice(&entropy_seed.to_le_bytes());
        self.port.write_all(&cmd)?;
        self.port.flush()?;

        let mut resp = [0u8; 5]; // 1 byte status + 4 byte key
        self.port
            .read_exact(&mut resp)
            .context("timeout waiting for provision response")?;

        if resp[0] == RESP_NAK {
            bail!("device NAK'd provision command");
        }
        let key = u32::from_le_bytes([resp[1], resp[2], resp[3], resp[4]]);
        Ok(key)
    }

    /// Read the current device key.
    pub fn read_key(&mut self) -> Result<Option<u32>> {
        self.port.write_all(&[CMD_READ_KEY])?;
        self.port.flush()?;

        let mut resp = [0u8; 5];
        self.port
            .read_exact(&mut resp)
            .context("timeout waiting for read-key response")?;

        if resp[0] == RESP_NAK {
            return Ok(None);
        }
        let key = u32::from_le_bytes([resp[1], resp[2], resp[3], resp[4]]);
        if key == 0 {
            Ok(None)
        } else {
            Ok(Some(key))
        }
    }

    /// Clear the device key.
    pub fn clear_key(&mut self) -> Result<bool> {
        self.port.write_all(&[CMD_CLEAR_KEY])?;
        self.port.flush()?;

        let mut resp = [0u8; 1];
        self.port
            .read_exact(&mut resp)
            .context("timeout waiting for clear response")?;
        Ok(resp[0] == RESP_ACK)
    }
}

/// Format a device key as "XX-YYYYYY" hex display.
pub fn format_device_key(key: u32) -> String {
    let prefix = (key >> 24) as u8;
    let suffix = key & 0x00FF_FFFF;
    format!("{:02X}-{:06X}", prefix, suffix)
}
