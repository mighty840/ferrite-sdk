use crate::chunks::ChunkEncoder;
use crate::fault::RamRegion;
use crate::metrics::MetricsBuffer;
use crate::trace::TraceBuffer;
use crate::SdkError;
use core::cell::RefCell;
use critical_section::Mutex;

/// SDK configuration provided by the user at init time.
pub struct SdkConfig<'a> {
    /// Device identifier string.
    pub device_id: &'a str,
    /// Firmware version string.
    pub firmware_version: &'a str,
    /// First 8 bytes of the ELF .build_id section.
    pub build_id: u64,
    /// Function that returns current monotonic tick count.
    pub ticks_fn: fn() -> u64,
    /// Valid RAM regions for fault handler stack snapshot safety.
    pub ram_regions: &'a [RamRegion],
}

/// Internal SDK state.
pub struct SdkState {
    pub device_id: &'static str,
    pub firmware_version: &'static str,
    pub build_id: u64,
    pub metrics: MetricsBuffer<32>,
    pub trace: TraceBuffer<512>,
    pub encoder: ChunkEncoder,
}

static SDK: Mutex<RefCell<Option<SdkState>>> = Mutex::new(RefCell::new(None));

use core::sync::atomic::{AtomicBool, Ordering};
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize the SDK. Call once at firmware startup.
///
/// # Panics
/// Panics if called more than once.
pub fn init(config: SdkConfig<'static>) {
    if INITIALIZED.load(Ordering::Relaxed) {
        panic!("ferrite-sdk: init() called more than once");
    }

    // Register RAM regions
    for region in config.ram_regions {
        let _ = crate::fault::register_ram_region(region.start, region.end);
    }

    // Validate retained RAM block
    unsafe {
        let retained = crate::memory::get_retained_block_ptr();
        if !(*retained).header.is_valid() {
            // First boot or corrupted — initialize
            (*retained).header.magic = crate::memory::RETAINED_MAGIC;
            (*retained).header.sequence = 0;
        }
        // Increment boot sequence
        (*retained).header.sequence = (*retained).header.sequence.wrapping_add(1);
    }

    // Set ticks function
    crate::metrics::set_ticks_fn(config.ticks_fn);

    let state = SdkState {
        device_id: config.device_id,
        firmware_version: config.firmware_version,
        build_id: config.build_id,
        metrics: MetricsBuffer::new(),
        trace: TraceBuffer::new(),
        encoder: ChunkEncoder::new(),
    };

    critical_section::with(|cs| {
        SDK.borrow(cs).replace(Some(state));
    });

    INITIALIZED.store(true, Ordering::Release);
}

/// Returns true if init() has been called.
pub fn is_initialized() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}

/// Access the SDK state in a critical section.
/// Returns the result of the closure, or panics if SDK is not initialized.
pub fn with_sdk<F, R>(f: F) -> R
where
    F: FnOnce(&mut SdkState) -> R,
{
    critical_section::with(|cs| {
        let mut borrow = SDK.borrow(cs).borrow_mut();
        let state = borrow
            .as_mut()
            .unwrap_or_else(|| panic!("ferrite-sdk: SDK not initialized"));
        f(state)
    })
}

/// Try to access the SDK state. Returns Err if not initialized.
pub fn try_with_sdk<F, R>(f: F) -> Result<R, SdkError>
where
    F: FnOnce(&mut SdkState) -> R,
{
    critical_section::with(|cs| {
        let mut borrow = SDK.borrow(cs).borrow_mut();
        match borrow.as_mut() {
            Some(state) => Ok(f(state)),
            None => Err(SdkError::NotInitialized),
        }
    })
}
