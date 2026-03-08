#![no_std]

use iotai_sdk::transport::ChunkTransport;
use iotai_sdk::upload::{UploadError, UploadManager, UploadStats};

/// Convenience wrapper that runs a full blocking upload session.
///
/// This delegates to [`UploadManager::upload`] and is intended to be called
/// from an RTIC task (either software or hardware) that has exclusive access
/// to the transport peripheral.
///
/// # Errors
///
/// Returns [`UploadError`] if the SDK is not initialized, the transport is
/// unavailable, or a transport / encoding error occurs during the session.
pub fn upload_blocking<T: ChunkTransport>(
    transport: &mut T,
) -> Result<UploadStats, UploadError<T::Error>> {
    UploadManager::upload(transport)
}

/// An RTIC-friendly resource wrapper around a [`ChunkTransport`].
///
/// In RTIC applications, shared resources must be locked before access.
/// `RticTransportResource` holds the transport and a pending flag so that one
/// task can *request* an upload (setting the flag) while a lower-priority
/// software task can *poll* to execute the upload when it is scheduled.
///
/// # Usage
///
/// ```ignore
/// // In RTIC shared resources:
/// struct Shared {
///     uploader: RticTransportResource<MyTransport>,
/// }
///
/// // Higher-priority task requests an upload:
/// #[task(shared = [uploader])]
/// fn periodic(mut cx: periodic::Context) {
///     cx.shared.uploader.lock(|u| u.request_upload());
///     // Pend the upload software task
///     upload_task::spawn().ok();
/// }
///
/// // Lower-priority software task performs the upload:
/// #[task(shared = [uploader])]
/// fn upload_task(mut cx: upload_task::Context) {
///     cx.shared.uploader.lock(|u| {
///         if let Some(result) = u.poll() {
///             match result {
///                 Ok(stats) => { /* handle stats */ }
///                 Err(e) => { /* handle error */ }
///             }
///         }
///     });
/// }
/// ```
pub struct RticTransportResource<T: ChunkTransport> {
    transport: T,
    pending: bool,
}

impl<T: ChunkTransport> RticTransportResource<T> {
    /// Create a new resource wrapping the given transport.
    ///
    /// The pending flag starts as `false` — no upload will occur until
    /// [`request_upload`](Self::request_upload) is called.
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            pending: false,
        }
    }

    /// Mark an upload as pending.
    ///
    /// The next call to [`poll`](Self::poll) will execute a blocking upload
    /// session. Calling this multiple times before `poll` is harmless — only
    /// one upload session will run.
    pub fn request_upload(&mut self) {
        self.pending = true;
    }

    /// If an upload is pending and the transport is available, run a blocking
    /// upload session and return the result.
    ///
    /// Returns `None` if no upload was requested (the pending flag is not set).
    ///
    /// On success or on a non-retryable error, the pending flag is cleared.
    /// On [`UploadError::TransportUnavailable`], the pending flag is kept so
    /// the upload will be retried on the next poll.
    pub fn poll(&mut self) -> Option<Result<UploadStats, UploadError<T::Error>>> {
        if !self.pending {
            return None;
        }

        let result = UploadManager::upload(&mut self.transport);

        match &result {
            Ok(_) => {
                self.pending = false;
            }
            Err(UploadError::TransportUnavailable) => {
                // Keep pending — transport may become available later.
            }
            Err(_) => {
                // Non-retryable error (encoding, not-initialized, transport
                // failure mid-session). Clear pending to avoid tight retry
                // loops; the caller can re-request if desired.
                self.pending = false;
            }
        }

        Some(result)
    }

    /// Returns `true` if an upload has been requested but not yet executed.
    pub fn is_pending(&self) -> bool {
        self.pending
    }

    /// Obtain a shared reference to the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Obtain an exclusive reference to the underlying transport.
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// Consume the resource and return the inner transport.
    pub fn into_inner(self) -> T {
        self.transport
    }
}
