//! Embassy helpers for periodic and on-demand Ferrite data upload.
//!
//! These are generic async functions intended to be called from user-defined
//! `#[embassy_executor::task]` functions. Embassy tasks require concrete
//! types, so users wrap these in their own task with their specific transport.
//!
//! # Example
//!
//! ```ignore
//! #[embassy_executor::task]
//! async fn upload_task(transport: MyUartTransport) -> ! {
//!     ferrite_embassy::upload_task::upload_loop(transport, Duration::from_secs(60)).await
//! }
//! ```

use embassy_futures::select::select;
#[cfg(feature = "defmt")]
use embassy_futures::select::Either;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use ferrite_sdk::transport::AsyncChunkTransport;
use ferrite_sdk::upload::UploadManager;

/// Channel used to trigger an immediate upload from any context.
///
/// Capacity of 1: multiple rapid triggers coalesce into a single upload.
static UPLOAD_NOW: Channel<CriticalSectionRawMutex, (), 1> = Channel::new();

/// Request an immediate upload cycle.
///
/// Safe to call from any context (interrupts, other tasks, etc.).
/// If a trigger is already pending, this silently succeeds.
pub fn trigger_upload_now() {
    let _ = UPLOAD_NOW.try_send(());
}

/// Periodic upload loop. Runs forever, uploading every `interval`.
///
/// Call this from your own `#[embassy_executor::task]` function.
pub async fn upload_loop<T: AsyncChunkTransport>(
    mut transport: T,
    interval: Duration,
) -> ! {
    loop {
        Timer::after(interval).await;

        match UploadManager::upload_async(&mut transport).await {
            Ok(_stats) => {
                #[cfg(feature = "defmt")]
                defmt::info!(
                    "ferrite upload ok: {} chunks, {} bytes",
                    _stats.chunks_sent,
                    _stats.bytes_sent,
                );
            }
            Err(_e) => {
                #[cfg(feature = "defmt")]
                defmt::warn!("ferrite upload failed: {}", defmt::Debug2Format(&_e));
            }
        }
    }
}

/// Upload loop that responds to both a periodic interval and an immediate
/// trigger via [`trigger_upload_now`].
///
/// Whichever fires first causes an upload attempt. Call this from your own
/// `#[embassy_executor::task]` function.
pub async fn upload_loop_with_trigger<T: AsyncChunkTransport>(
    mut transport: T,
    interval: Duration,
) -> ! {
    loop {
        let _reason = select(Timer::after(interval), UPLOAD_NOW.receive()).await;

        #[cfg(feature = "defmt")]
        match &_reason {
            Either::First(()) => {
                defmt::trace!("ferrite upload: periodic interval elapsed");
            }
            Either::Second(()) => {
                defmt::trace!("ferrite upload: triggered by UPLOAD_NOW");
            }
        }
        let _ = &_reason;

        match UploadManager::upload_async(&mut transport).await {
            Ok(_stats) => {
                #[cfg(feature = "defmt")]
                defmt::info!(
                    "ferrite upload ok: {} chunks, {} bytes",
                    _stats.chunks_sent,
                    _stats.bytes_sent,
                );
            }
            Err(_e) => {
                #[cfg(feature = "defmt")]
                defmt::warn!("ferrite upload failed: {}", defmt::Debug2Format(&_e));
            }
        }

        // Drain any extra triggers that arrived while uploading.
        while UPLOAD_NOW.try_receive().is_ok() {}
    }
}
