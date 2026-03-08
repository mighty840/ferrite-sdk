// defmt global logger implementation for ferrite-sdk.
// Feature-gated behind `defmt`.
//
// Writes defmt-encoded frames into the SDK trace buffer.
// All defmt frames are written with level=5 (sdk_internal).

// This module is only compiled when the defmt feature is enabled.
// The actual defmt Logger implementation requires careful integration
// with the defmt crate's global_logger! macro.

// For now, this is a placeholder. The full implementation requires:
// 1. A staging buffer to accumulate defmt::write() calls
// 2. Critical section management for acquire/release
// 3. Flushing the staging buffer as a trace frame on release
//
// The implementation is deferred because it requires the defmt crate
// to be available, which is feature-gated.

#[cfg(feature = "defmt")]
mod sink {
    use core::sync::atomic::{AtomicBool, Ordering};

    static LOGGING_ACTIVE: AtomicBool = AtomicBool::new(false);
    static mut STAGING_BUF: [u8; 128] = [0; 128];
    static mut STAGING_POS: usize = 0;

    #[defmt::global_logger]
    struct IotaiDefmtSink;

    unsafe impl defmt::Logger for IotaiDefmtSink {
        fn acquire() {
            // Prevent reentrant logging
            if LOGGING_ACTIVE.swap(true, Ordering::Acquire) {
                return;
            }
            unsafe {
                STAGING_POS = 0;
            }
        }

        unsafe fn flush() {
            // Write the staged bytes as a complete trace frame
            let pos = STAGING_POS;
            if pos > 0 {
                let payload = &STAGING_BUF[..pos];
                let ticks = (crate::metrics::ticks() & 0xFFFF_FFFF) as u32;
                crate::sdk::try_with_sdk(|sdk| {
                    sdk.trace.write_frame(5, ticks, payload);
                }).ok();
            }
        }

        unsafe fn release() {
            // Flush and exit critical section
            Self::flush();
            LOGGING_ACTIVE.store(false, Ordering::Release);
        }

        unsafe fn write(bytes: &[u8]) {
            let pos = STAGING_POS;
            let remaining = 128 - pos;
            let to_copy = bytes.len().min(remaining);
            if to_copy > 0 {
                STAGING_BUF[pos..pos + to_copy].copy_from_slice(&bytes[..to_copy]);
                STAGING_POS = pos + to_copy;
            }
        }
    }
}
