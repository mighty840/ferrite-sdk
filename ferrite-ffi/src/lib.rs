#![no_std]

use core::ffi::c_char;
use core::slice;

use ferrite_sdk::fault::{ExceptionFrame, ExtendedRegisters, FaultRecord, FaultType, RamRegion};
use ferrite_sdk::reboot_reason::RebootReason;
use ferrite_sdk::sdk::SdkConfig;
use ferrite_sdk::transport::ChunkTransport;
use ferrite_sdk::upload::{UploadManager, UploadStats};

// ---------------------------------------------------------------------------
// Error enum
// ---------------------------------------------------------------------------

/// Error codes returned by all FFI functions.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IotaiError {
    Ok = 0,
    NotInitialized = -1,
    AlreadyInit = -2,
    BufferFull = -3,
    KeyTooLong = -4,
    NullPtr = -5,
    Encoding = -6,
    Transport = -7,
}

impl From<ferrite_sdk::SdkError> for IotaiError {
    fn from(e: ferrite_sdk::SdkError) -> Self {
        match e {
            ferrite_sdk::SdkError::NotInitialized => IotaiError::NotInitialized,
            ferrite_sdk::SdkError::AlreadyInitialized => IotaiError::AlreadyInit,
            ferrite_sdk::SdkError::BufferFull => IotaiError::BufferFull,
            ferrite_sdk::SdkError::KeyTooLong => IotaiError::KeyTooLong,
            ferrite_sdk::SdkError::EncodingFailed => IotaiError::Encoding,
            _ => IotaiError::Encoding,
        }
    }
}

// ---------------------------------------------------------------------------
// Function-pointer type aliases for the C transport
// ---------------------------------------------------------------------------

/// Callback to send a single chunk.
/// `data` points to chunk bytes, `len` is the byte count.
/// Returns 0 on success, non-zero on error.
pub type IotaiSendChunkFn =
    Option<unsafe extern "C" fn(data: *const u8, len: u32, ctx: *mut core::ffi::c_void) -> i32>;

/// Callback to query transport availability.
/// Returns `true` (non-zero) if the link is ready.
pub type IotaiIsAvailableFn = Option<unsafe extern "C" fn(ctx: *mut core::ffi::c_void) -> bool>;

// ---------------------------------------------------------------------------
// IotaiTransport — C-visible transport descriptor
// ---------------------------------------------------------------------------

/// Transport descriptor passed from C.
/// The caller owns the `ctx` pointer and the function pointers must remain
/// valid for the duration of any `ferrite_upload` call.
#[repr(C)]
pub struct IotaiTransport {
    /// Required: sends a single chunk.  Returns 0 on success.
    pub send_chunk: IotaiSendChunkFn,
    /// Optional: returns whether the transport is available.
    /// If NULL the transport is assumed to always be available.
    pub is_available: IotaiIsAvailableFn,
    /// Opaque context pointer forwarded to every callback.
    pub ctx: *mut core::ffi::c_void,
}

// SAFETY: The C side is responsible for ensuring that the function pointers
// and context pointer are safe to call/access from any context the FFI
// functions are invoked from.  On bare-metal Cortex-M there is only one
// core, so Send+Sync are trivially satisfied.
unsafe impl Send for IotaiTransport {}
unsafe impl Sync for IotaiTransport {}

// ---------------------------------------------------------------------------
// IotaiRamRegion — C-visible RAM region
// ---------------------------------------------------------------------------

/// A valid RAM address range for fault-handler stack snapshot validation.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IotaiRamRegion {
    pub start: u32,
    pub end: u32,
}

// ---------------------------------------------------------------------------
// FfiTransport — adapts C callbacks to the Rust ChunkTransport trait
// ---------------------------------------------------------------------------

/// Internal adapter that implements `ChunkTransport` by forwarding to C
/// function pointers stored in an `IotaiTransport`.
struct FfiTransport {
    send_chunk_fn:
        unsafe extern "C" fn(data: *const u8, len: u32, ctx: *mut core::ffi::c_void) -> i32,
    is_available_fn: Option<unsafe extern "C" fn(ctx: *mut core::ffi::c_void) -> bool>,
    ctx: *mut core::ffi::c_void,
}

/// Error type surfaced by the FFI transport adapter.
#[derive(Debug)]
pub struct FfiTransportError(i32);

impl ChunkTransport for FfiTransport {
    type Error = FfiTransportError;

    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        let rc = unsafe { (self.send_chunk_fn)(chunk.as_ptr(), chunk.len() as u32, self.ctx) };
        if rc == 0 {
            Ok(())
        } else {
            Err(FfiTransportError(rc))
        }
    }

    fn is_available(&self) -> bool {
        match self.is_available_fn {
            Some(f) => unsafe { f(self.ctx) },
            None => true,
        }
    }
}

// ---------------------------------------------------------------------------
// ferrite_sdk_init
// ---------------------------------------------------------------------------

/// Initialise the SDK.
///
/// # Parameters
/// * `device_id`        – NUL-terminated device identifier.
/// * `firmware_version` – NUL-terminated firmware version string.
/// * `build_id`         – First 8 bytes of the ELF `.build_id`.
/// * `ticks_fn`         – Function returning the current monotonic tick count.
/// * `ram_regions`      – Pointer to an array of `IotaiRamRegion`.
/// * `ram_region_count` – Number of elements in the array.
///
/// Returns `IotaiError::Ok` on success.
#[no_mangle]
pub unsafe extern "C" fn ferrite_sdk_init(
    device_id: *const c_char,
    firmware_version: *const c_char,
    build_id: u64,
    ticks_fn: Option<extern "C" fn() -> u64>,
    ram_regions: *const IotaiRamRegion,
    ram_region_count: u32,
) -> IotaiError {
    // Validate required pointers.
    if device_id.is_null() || firmware_version.is_null() {
        return IotaiError::NullPtr;
    }

    if ticks_fn.is_none() {
        return IotaiError::NullPtr;
    }

    if ferrite_sdk::sdk::is_initialized() {
        return IotaiError::AlreadyInit;
    }

    // Convert C strings to &'static str.
    // SAFETY: The caller guarantees the pointers are valid NUL-terminated
    // strings whose backing memory lives for the duration of the program
    // (typically string literals in .rodata).
    let dev_id = cstr_to_str(device_id);
    let fw_ver = cstr_to_str(firmware_version);

    // Build the RAM regions slice.
    let regions: &[IotaiRamRegion] = if ram_regions.is_null() || ram_region_count == 0 {
        &[]
    } else {
        slice::from_raw_parts(ram_regions, ram_region_count as usize)
    };

    // Convert IotaiRamRegion to RamRegion.
    // We support up to 4 regions (same as the SDK limit).
    static mut RAM_REGION_BUF: [RamRegion; 4] = [RamRegion { start: 0, end: 0 }; 4];
    let count = regions.len().min(4);
    for i in 0..count {
        RAM_REGION_BUF[i] = RamRegion {
            start: regions[i].start,
            end: regions[i].end,
        };
    }
    let sdk_regions: &'static [RamRegion] = &RAM_REGION_BUF[..count];

    // Wrap the C ticks function so it matches the expected Rust signature.
    // We store the function pointer in a static so we can create a plain `fn`
    // pointer (no closure captures).
    static mut C_TICKS_FN: Option<extern "C" fn() -> u64> = None;
    C_TICKS_FN = ticks_fn;

    fn rust_ticks() -> u64 {
        // SAFETY: Written once before init, read only after.
        unsafe {
            match C_TICKS_FN {
                Some(f) => f(),
                None => 0,
            }
        }
    }

    let config = SdkConfig {
        device_id: dev_id,
        firmware_version: fw_ver,
        build_id,
        ticks_fn: rust_ticks,
        ram_regions: sdk_regions,
    };

    ferrite_sdk::sdk::init(config);

    IotaiError::Ok
}

/// Convert a C NUL-terminated string to a `&'static str`.
///
/// # Safety
/// The pointer must be non-null, valid, and point to a NUL-terminated string
/// in memory that lives for the remainder of the program.
unsafe fn cstr_to_str(ptr: *const c_char) -> &'static str {
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    let bytes = slice::from_raw_parts(ptr as *const u8, len);
    // We trust that the caller provides valid UTF-8 (ASCII device ids / versions).
    // If not, we use the unchecked variant to avoid pulling in the formatting
    // machinery on no_std targets.
    core::str::from_utf8_unchecked(bytes)
}

// ---------------------------------------------------------------------------
// Reboot reason
// ---------------------------------------------------------------------------

/// Record the reboot reason for the current boot cycle.
///
/// `reason` maps to `RebootReason` discriminants (0 = Unknown, 1 = PowerOnReset, ...).
#[no_mangle]
pub unsafe extern "C" fn ferrite_record_reboot_reason(reason: u8) -> IotaiError {
    if !ferrite_sdk::sdk::is_initialized() {
        return IotaiError::NotInitialized;
    }
    ferrite_sdk::reboot_reason::record_reboot_reason(RebootReason::from(reason));
    IotaiError::Ok
}

/// Retrieve the reboot reason recorded by the previous boot.
///
/// On success the reason discriminant is written to `*out_reason` and
/// `IotaiError::Ok` is returned.  If no valid record exists the function
/// still returns `Ok` but writes `0` (Unknown).
#[no_mangle]
pub unsafe extern "C" fn ferrite_last_reboot_reason(out_reason: *mut u8) -> IotaiError {
    if out_reason.is_null() {
        return IotaiError::NullPtr;
    }
    match ferrite_sdk::reboot_reason::last_reboot_reason() {
        Some(r) => {
            *out_reason = r as u8;
        }
        None => {
            *out_reason = RebootReason::Unknown as u8;
        }
    }
    IotaiError::Ok
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// Increment a counter metric by `delta`.
///
/// `key` is a NUL-terminated C string (max 32 characters).
#[no_mangle]
pub unsafe extern "C" fn ferrite_metric_increment(key: *const c_char, delta: u32) -> IotaiError {
    if key.is_null() {
        return IotaiError::NullPtr;
    }
    if !ferrite_sdk::sdk::is_initialized() {
        return IotaiError::NotInitialized;
    }

    let key_str = cstr_to_str(key);
    let ticks = ferrite_sdk::metrics::ticks();
    match ferrite_sdk::sdk::try_with_sdk(|state| state.metrics.increment(key_str, delta, ticks)) {
        Ok(Ok(())) => IotaiError::Ok,
        Ok(Err(e)) => IotaiError::from(e),
        Err(e) => IotaiError::from(e),
    }
}

/// Set a gauge metric to `value`.
///
/// `key` is a NUL-terminated C string (max 32 characters).
#[no_mangle]
pub unsafe extern "C" fn ferrite_metric_gauge(key: *const c_char, value: f32) -> IotaiError {
    if key.is_null() {
        return IotaiError::NullPtr;
    }
    if !ferrite_sdk::sdk::is_initialized() {
        return IotaiError::NotInitialized;
    }

    let key_str = cstr_to_str(key);
    let ticks = ferrite_sdk::metrics::ticks();
    match ferrite_sdk::sdk::try_with_sdk(|state| state.metrics.gauge(key_str, value, ticks)) {
        Ok(Ok(())) => IotaiError::Ok,
        Ok(Err(e)) => IotaiError::from(e),
        Err(e) => IotaiError::from(e),
    }
}

/// Record a histogram observation for `key`.
///
/// `key` is a NUL-terminated C string (max 32 characters).
#[no_mangle]
pub unsafe extern "C" fn ferrite_metric_observe(key: *const c_char, value: f32) -> IotaiError {
    if key.is_null() {
        return IotaiError::NullPtr;
    }
    if !ferrite_sdk::sdk::is_initialized() {
        return IotaiError::NotInitialized;
    }

    let key_str = cstr_to_str(key);
    let ticks = ferrite_sdk::metrics::ticks();
    match ferrite_sdk::sdk::try_with_sdk(|state| state.metrics.observe(key_str, value, ticks)) {
        Ok(Ok(())) => IotaiError::Ok,
        Ok(Err(e)) => IotaiError::from(e),
        Err(e) => IotaiError::from(e),
    }
}

// ---------------------------------------------------------------------------
// Fault record
// ---------------------------------------------------------------------------

/// C-visible fault record returned by `ferrite_last_fault`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IotaiFaultRecord {
    /// `true` if a valid fault was present.
    pub valid: bool,
    /// Fault type (0 = HardFault, 1 = MemFault, 2 = BusFault, 3 = UsageFault).
    pub fault_type: u8,
    pub _pad: [u8; 2],
    /// Hardware exception frame registers.
    pub r0: u32,
    pub r1: u32,
    pub r2: u32,
    pub r3: u32,
    pub r12: u32,
    pub lr: u32,
    pub pc: u32,
    pub xpsr: u32,
    /// Extended (software-captured) registers.
    pub r4: u32,
    pub r5: u32,
    pub r6: u32,
    pub r7: u32,
    pub r8: u32,
    pub r9: u32,
    pub r10: u32,
    pub r11: u32,
    pub sp: u32,
    /// First 16 words above SP at fault time.
    pub stack_snapshot: [u32; 16],
    /// Cortex-M fault status registers.
    pub cfsr: u32,
    pub hfsr: u32,
    pub mmfar: u32,
    pub bfar: u32,
}

impl IotaiFaultRecord {
    /// Create a zeroed (invalid) fault record.
    pub const fn zeroed() -> Self {
        Self {
            valid: false,
            fault_type: 0,
            _pad: [0; 2],
            r0: 0,
            r1: 0,
            r2: 0,
            r3: 0,
            r12: 0,
            lr: 0,
            pc: 0,
            xpsr: 0,
            r4: 0,
            r5: 0,
            r6: 0,
            r7: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            sp: 0,
            stack_snapshot: [0; 16],
            cfsr: 0,
            hfsr: 0,
            mmfar: 0,
            bfar: 0,
        }
    }

    /// Convert from the Rust SDK `FaultRecord`.
    fn from_sdk(r: &FaultRecord) -> Self {
        Self {
            valid: r.valid,
            fault_type: r.fault_type as u8,
            _pad: [0; 2],
            r0: r.frame.r0,
            r1: r.frame.r1,
            r2: r.frame.r2,
            r3: r.frame.r3,
            r12: r.frame.r12,
            lr: r.frame.lr,
            pc: r.frame.pc,
            xpsr: r.frame.xpsr,
            r4: r.extended.r4,
            r5: r.extended.r5,
            r6: r.extended.r6,
            r7: r.extended.r7,
            r8: r.extended.r8,
            r9: r.extended.r9,
            r10: r.extended.r10,
            r11: r.extended.r11,
            sp: r.extended.sp,
            stack_snapshot: r.stack_snapshot,
            cfsr: r.cfsr,
            hfsr: r.hfsr,
            mmfar: r.mmfar,
            bfar: r.bfar,
        }
    }
}

/// Retrieve the fault record from the previous boot.
///
/// The fault record is written to `*out`.  If no valid fault is present
/// `out->valid` will be `false`.
#[no_mangle]
pub unsafe extern "C" fn ferrite_last_fault(out: *mut IotaiFaultRecord) -> IotaiError {
    if out.is_null() {
        return IotaiError::NullPtr;
    }

    match ferrite_sdk::fault::last_fault() {
        Some(record) => {
            *out = IotaiFaultRecord::from_sdk(&record);
        }
        None => {
            *out = IotaiFaultRecord::zeroed();
        }
    }

    IotaiError::Ok
}

// ---------------------------------------------------------------------------
// Upload
// ---------------------------------------------------------------------------

/// Statistics returned by `ferrite_upload`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IotaiUploadStats {
    pub chunks_sent: u32,
    pub bytes_sent: u32,
    pub fault_uploaded: bool,
    pub metrics_uploaded: u32,
    pub trace_bytes_uploaded: u32,
}

impl IotaiUploadStats {
    pub const fn zeroed() -> Self {
        Self {
            chunks_sent: 0,
            bytes_sent: 0,
            fault_uploaded: false,
            metrics_uploaded: 0,
            trace_bytes_uploaded: 0,
        }
    }

    fn from_sdk(s: &UploadStats) -> Self {
        Self {
            chunks_sent: s.chunks_sent,
            bytes_sent: s.bytes_sent,
            fault_uploaded: s.fault_uploaded,
            metrics_uploaded: s.metrics_uploaded,
            trace_bytes_uploaded: s.trace_bytes_uploaded,
        }
    }
}

/// Run a full blocking upload session using the provided transport.
///
/// On success, upload statistics are written to `*out_stats` (may be NULL if
/// the caller does not need them).
///
/// On transport error `IotaiError::Transport` is returned and buffered data
/// is retained for the next attempt.
#[no_mangle]
pub unsafe extern "C" fn ferrite_upload(
    transport: *const IotaiTransport,
    out_stats: *mut IotaiUploadStats,
) -> IotaiError {
    if transport.is_null() {
        return IotaiError::NullPtr;
    }

    let t = &*transport;

    // The send_chunk callback is mandatory.
    let send_fn = match t.send_chunk {
        Some(f) => f,
        None => return IotaiError::NullPtr,
    };

    let mut adapter = FfiTransport {
        send_chunk_fn: send_fn,
        is_available_fn: t.is_available.map(|f| f),
        ctx: t.ctx,
    };

    match UploadManager::upload(&mut adapter) {
        Ok(stats) => {
            if !out_stats.is_null() {
                *out_stats = IotaiUploadStats::from_sdk(&stats);
            }
            IotaiError::Ok
        }
        Err(ferrite_sdk::upload::UploadError::NotInitialized) => IotaiError::NotInitialized,
        Err(ferrite_sdk::upload::UploadError::TransportUnavailable) => IotaiError::Transport,
        Err(ferrite_sdk::upload::UploadError::TransportError(_)) => IotaiError::Transport,
        Err(ferrite_sdk::upload::UploadError::EncodingError) => IotaiError::Encoding,
    }
}

// ---------------------------------------------------------------------------
// Panic handler (required for no_std staticlib)
// ---------------------------------------------------------------------------

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
