//! Signed OTA for the H563 via embassy-boot.
//!
//! Flow:
//! 1. `poll_and_update` asks the gateway's OTA relay for this device's target.
//! 2. If there is one, the image streams into the DFU slot (bank 2) while the
//!    SDK verifies the signed manifest first and size/hash as it goes.
//! 3. Only a fully verified image is marked updated; then the board resets
//!    and the bootloader swaps it in.
//! 4. The new image calls `confirm` once it has proven itself (network up
//!    and one successful upload). Until then the bootloader state stays
//!    "swap": any reset, including the watchdog firing on a hung image,
//!    makes the bootloader swap the old image back.

use core::cell::RefCell;

use embassy_boot_stm32::{
    AlignedBuffer, BlockingFirmwareUpdater, FirmwareUpdaterConfig, State,
};
use embassy_net::tcp::TcpSocket;
use embassy_net::{IpEndpoint, Stack};
use embassy_stm32::flash::{Blocking, Flash, WRITE_SIZE};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::Duration;
use embedded_storage::nor_flash::NorFlash;
use ferrite_sdk::ota::{OtaCheckResult, OtaClient};
use ferrite_sdk::ota_manifest::UpdatePolicy;

use crate::identity::{BUILD_ID, OTA_TRUSTED_KEYS, SECURITY_VERSION};

/// Size of the ACTIVE partition; an image must fit (see memory.x).
const ACTIVE_SIZE: u32 = 256 * 1024;

pub type FlashMutex = Mutex<NoopRawMutex, RefCell<Flash<'static, Blocking>>>;

/// Create the OTA handler over the DFU and state partitions from memory.x.
pub fn new(
    flash: &'static FlashMutex,
    state_buf: &'static mut AlignedBuffer<WRITE_SIZE>,
) -> Ota<impl NorFlash, impl NorFlash> {
    let config = FirmwareUpdaterConfig::from_linkerfile_blocking(flash, flash);
    Ota::from_updater(BlockingFirmwareUpdater::new(config, &mut state_buf.0))
}

pub struct Ota<DFU: NorFlash, STATE: NorFlash> {
    updater: BlockingFirmwareUpdater<'static, DFU, STATE>,
    /// Running a freshly swapped image that hasn't been confirmed yet.
    pending_confirm: bool,
}

impl<DFU: NorFlash, STATE: NorFlash> Ota<DFU, STATE> {
    fn from_updater(mut updater: BlockingFirmwareUpdater<'static, DFU, STATE>) -> Self {
        let state = updater.get_state();
        let pending_confirm = matches!(state, Ok(State::Swap));
        match state {
            Ok(State::Swap) => defmt::warn!("OTA: new image, unconfirmed (reset now = rollback)"),
            Ok(State::Revert) => defmt::warn!("OTA: previous update was rolled back"),
            Ok(_) => {}
            Err(_) => defmt::error!("OTA: cannot read bootloader state"),
        }
        Self {
            updater,
            pending_confirm,
        }
    }

    pub fn pending_confirm(&self) -> bool {
        self.pending_confirm
    }

    /// Keep the swapped-in image. Call only after the self-test passed.
    pub fn confirm(&mut self) {
        if !self.pending_confirm {
            return;
        }
        match self.updater.mark_booted() {
            Ok(()) => {
                self.pending_confirm = false;
                defmt::info!("OTA: update confirmed, build_id={}", BUILD_ID);
                let _ = ferrite_sdk::metric_increment!("ota_confirmed");
            }
            Err(_) => defmt::error!("OTA: mark_booted failed"),
        }
    }

    /// Check for an update and install it. Returns only if there was nothing
    /// to do or the update failed; a verified update resets the board.
    pub async fn poll_and_update(
        &mut self,
        stack: Stack<'static>,
        relay: IpEndpoint,
        relay_url: &'static str,
        device_id: &'static str,
        pet: &mut impl FnMut(),
    ) {
        // Don't stack a second update on an unconfirmed one.
        if self.pending_confirm {
            return;
        }
        let client = OtaClient::new(device_id, BUILD_ID, relay_url, None);

        let mut rx = [0u8; 1536];
        let mut tx = [0u8; 512];
        let target = {
            let mut sock = TcpSocket::new(stack, &mut rx, &mut tx);
            sock.set_timeout(Some(Duration::from_secs(10)));
            if sock.connect(relay).await.is_err() {
                return;
            }
            let result = client.check(&mut sock).await;
            sock.close();
            match result {
                Ok(OtaCheckResult::UpToDate) => return,
                Ok(OtaCheckResult::UpdateAvailable(t)) => t,
                Err(e) => {
                    defmt::warn!("OTA check failed: {}", defmt::Debug2Format(&e));
                    return;
                }
            }
        };
        defmt::info!(
            "OTA: update to {} build {} available",
            target.version.as_str(),
            target.build_id
        );

        let policy = UpdatePolicy {
            trusted_keys: &OTA_TRUSTED_KEYS,
            min_security_version: SECURITY_VERSION,
        };
        let mut rx = [0u8; 4096];
        let mut tx = [0u8; 512];
        let mut sock = TcpSocket::new(stack, &mut rx, &mut tx);
        sock.set_timeout(Some(Duration::from_secs(20)));
        if sock.connect(relay).await.is_err() {
            return;
        }

        let mut writer = AlignedWriter::new(&mut self.updater);
        let result = client
            .download(&mut sock, &target, &policy, |offset, data| {
                pet();
                if offset + data.len() as u32 > ACTIVE_SIZE {
                    return Err(WriteError::TooLarge);
                }
                writer.write(data)
            })
            .await;
        sock.close();

        let finished = match result {
            Ok(manifest) => writer.finish().map(|()| manifest),
            Err(e) => {
                defmt::warn!("OTA download rejected: {}", defmt::Debug2Format(&e));
                let _ = ferrite_sdk::metric_increment!("ota_failed");
                return;
            }
        };
        let manifest = match finished {
            Ok(m) => m,
            Err(_) => {
                defmt::error!("OTA: flushing the update slot failed");
                return;
            }
        };

        if self.updater.mark_updated().is_err() {
            defmt::error!("OTA: mark_updated failed");
            return;
        }
        defmt::info!(
            "OTA: verified {} build {} (security_version {}), rebooting into it",
            manifest.version.as_str(),
            manifest.build_id,
            manifest.security_version
        );
        embassy_time::Timer::after_millis(200).await;
        cortex_m::peripheral::SCB::sys_reset();
    }
}

#[derive(Debug)]
pub enum WriteError {
    TooLarge,
    Flash,
}

/// Collects the arbitrary-length pieces a TCP read returns into
/// flash-write-size blocks for the DFU partition.
struct AlignedWriter<'u, DFU: NorFlash, STATE: NorFlash> {
    updater: &'u mut BlockingFirmwareUpdater<'static, DFU, STATE>,
    buf: [u8; WRITE_SIZE],
    len: usize,
    offset: usize,
}

impl<'u, DFU: NorFlash, STATE: NorFlash> AlignedWriter<'u, DFU, STATE> {
    fn new(updater: &'u mut BlockingFirmwareUpdater<'static, DFU, STATE>) -> Self {
        Self {
            updater,
            buf: [0xFF; WRITE_SIZE],
            len: 0,
            offset: 0,
        }
    }

    fn write(&mut self, mut data: &[u8]) -> Result<(), WriteError> {
        while !data.is_empty() {
            let n = (WRITE_SIZE - self.len).min(data.len());
            self.buf[self.len..self.len + n].copy_from_slice(&data[..n]);
            self.len += n;
            data = &data[n..];
            if self.len == WRITE_SIZE {
                self.flush_block()?;
            }
        }
        Ok(())
    }

    /// Write the last partial block, padded with erased-flash bytes.
    fn finish(mut self) -> Result<(), WriteError> {
        if self.len > 0 {
            self.buf[self.len..].fill(0xFF);
            self.flush_block()?;
        }
        Ok(())
    }

    fn flush_block(&mut self) -> Result<(), WriteError> {
        self.updater
            .write_firmware(self.offset, &self.buf)
            .map_err(|_| WriteError::Flash)?;
        self.offset += WRITE_SIZE;
        self.len = 0;
        self.buf = [0xFF; WRITE_SIZE];
        Ok(())
    }
}
