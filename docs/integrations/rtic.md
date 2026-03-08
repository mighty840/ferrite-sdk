# RTIC Integration

The `ferrite-rtic` crate provides a blocking upload helper and a shared-resource wrapper for RTIC v2 applications.

## Overview

RTIC uses priority-based preemptive scheduling. The ferrite-sdk RTIC integration provides two tools:

- **`upload_blocking()`** -- a thin wrapper around `UploadManager::upload()` for use in software tasks.
- **`RticTransportResource<T>`** -- a shared resource that holds your transport and a pending flag, enabling the request/poll pattern common in RTIC applications.

## Dependencies

```toml
[dependencies]
ferrite-sdk = { git = "https://github.com/mighty840/ferrite-sdk", features = ["cortex-m", "defmt"] }
ferrite-rtic = { git = "https://github.com/mighty840/ferrite-sdk" }
rtic = { version = "2", features = ["thumbv7-backend"] }
```

Note: the `embassy` feature is **not** needed for RTIC. The RTIC integration uses the blocking `ChunkTransport` trait.

## Simple pattern: upload_blocking

If you have a dedicated software task for uploads and can pass the transport directly:

```rust
#[rtic::app(device = nrf52840_hal::pac, dispatchers = [SWI0_EGU0])]
mod app {
    use ferrite_rtic::upload_blocking;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        transport: MyUartTransport,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        // ... init SDK, configure peripherals ...
        let transport = MyUartTransport::new(uart);
        upload_task::spawn_after(60.secs()).ok();
        (Shared {}, Local { transport })
    }

    #[task(local = [transport])]
    async fn upload_task(cx: upload_task::Context) {
        match upload_blocking(cx.local.transport) {
            Ok(stats) => {
                defmt::info!("upload ok: {} chunks", stats.chunks_sent);
            }
            Err(e) => {
                defmt::warn!("upload failed: {}", defmt::Debug2Format(&e));
            }
        }
        // Re-schedule
        upload_task::spawn_after(60.secs()).ok();
    }
}
```

## Advanced pattern: RticTransportResource

When the transport is a shared resource (e.g., a UART that is also used for other purposes), use `RticTransportResource`:

```rust
#[rtic::app(device = nrf52840_hal::pac, dispatchers = [SWI0_EGU0, SWI1_EGU1])]
mod app {
    use ferrite_rtic::RticTransportResource;

    #[shared]
    struct Shared {
        uploader: RticTransportResource<MyUartTransport>,
    }

    #[local]
    struct Local {}

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let transport = MyUartTransport::new(uart);
        let uploader = RticTransportResource::new(transport);

        // Schedule periodic upload requests
        periodic::spawn_after(60.secs()).ok();

        (Shared { uploader }, Local {})
    }

    /// Higher-priority task: requests an upload and pends the upload task.
    #[task(shared = [uploader], priority = 2)]
    async fn periodic(mut cx: periodic::Context) {
        cx.shared.uploader.lock(|u| u.request_upload());
        do_upload::spawn().ok();

        // Re-schedule
        periodic::spawn_after(60.secs()).ok();
    }

    /// Lower-priority software task: performs the actual upload.
    #[task(shared = [uploader], priority = 1)]
    async fn do_upload(mut cx: do_upload::Context) {
        cx.shared.uploader.lock(|u| {
            if let Some(result) = u.poll() {
                match result {
                    Ok(stats) => {
                        defmt::info!("upload ok: {} chunks", stats.chunks_sent);
                    }
                    Err(e) => {
                        defmt::warn!("upload failed: {}", defmt::Debug2Format(&e));
                    }
                }
            }
        });
    }
}
```

### How RticTransportResource works

- `request_upload()` sets an internal `pending` flag. This is cheap and can be called from any priority level.
- `poll()` checks the pending flag. If set, it calls `UploadManager::upload()` with the inner transport and returns `Some(result)`. If not set, it returns `None`.
- On success, the pending flag is cleared.
- On `TransportUnavailable`, the pending flag is **kept** so the upload retries on the next poll.
- On other errors (encoding, transport failure mid-session), the pending flag is cleared to avoid tight retry loops. The caller can re-request if desired.

### Accessing the underlying transport

If you need the transport for non-upload purposes:

```rust
cx.shared.uploader.lock(|u| {
    let transport = u.transport_mut();
    // Use transport for other UART operations
});
```

You can also consume the resource to recover the transport:

```rust
let transport = uploader.into_inner();
```

## Implementing ChunkTransport for RTIC

The blocking `ChunkTransport` trait is simpler than the async variant:

```rust
use ferrite_sdk::transport::ChunkTransport;
use nrf52840_hal::uarte::{self, Uarte};

pub struct MyUartTransport {
    uart: Uarte<nrf52840_hal::pac::UARTE0>,
}

impl ChunkTransport for MyUartTransport {
    type Error = uarte::Error;

    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        self.uart.write(chunk)?;
        Ok(())
    }
}
```

## Triggering an immediate upload

Unlike the Embassy integration (which has a built-in `trigger_upload_now()` channel), the RTIC pattern uses RTIC's own task spawning:

```rust
// From any task or interrupt handler:
do_upload::spawn().ok();

// Or from a shared resource context:
cx.shared.uploader.lock(|u| u.request_upload());
do_upload::spawn().ok();
```
