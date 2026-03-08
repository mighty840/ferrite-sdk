use crate::transport::ChunkTransport;
use crate::sdk;

/// Upload statistics returned after a successful upload session.
#[derive(Debug, Default)]
pub struct UploadStats {
    pub chunks_sent: u32,
    pub bytes_sent: u32,
    pub fault_uploaded: bool,
    pub metrics_uploaded: u32,
    pub trace_bytes_uploaded: u32,
}

/// Upload error type, generic over the transport error.
#[derive(Debug)]
pub enum UploadError<E> {
    TransportUnavailable,
    TransportError(E),
    EncodingError,
    NotInitialized,
}

/// Orchestrates a full upload session.
pub struct UploadManager;

impl UploadManager {
    /// Run a full blocking upload session.
    ///
    /// Upload order:
    /// 1. DeviceInfo (always)
    /// 2. RebootReason (if pending)
    /// 3. FaultRecord (if pending)
    /// 4. Metrics (if any)
    /// 5. TraceFragment chunks (if any)
    /// 6. Heartbeat (always, as final chunk)
    ///
    /// On transport error: abort session, retain data for next attempt.
    /// On success: clear uploaded buffers.
    pub fn upload<T: ChunkTransport>(transport: &mut T) -> Result<UploadStats, UploadError<T::Error>> {
        if !sdk::is_initialized() {
            return Err(UploadError::NotInitialized);
        }

        if !transport.is_available() {
            return Err(UploadError::TransportUnavailable);
        }

        transport.begin_session().map_err(UploadError::TransportError)?;

        let mut stats = UploadStats::default();
        let mut upload_ok = true;

        // Helper to send a chunk and track stats
        let send = |transport: &mut T, chunk: &[u8], stats: &mut UploadStats| -> Result<(), UploadError<T::Error>> {
            transport.send_chunk(chunk).map_err(UploadError::TransportError)?;
            stats.chunks_sent += 1;
            stats.bytes_sent += chunk.len() as u32;
            Ok(())
        };

        sdk::with_sdk(|state| -> Result<(), UploadError<T::Error>> {
            // 1. DeviceInfo
            state.encoder.encode_device_info(
                state.device_id,
                state.firmware_version,
                state.build_id,
                |chunk| {
                    if upload_ok {
                        if let Err(_) = send(transport, chunk, &mut stats) {
                            upload_ok = false;
                        }
                    }
                },
            );
            if !upload_ok {
                return Err(UploadError::EncodingError);
            }

            // 2. RebootReason (if pending)
            if let Some(reason) = crate::reboot_reason::last_reboot_reason() {
                let boot_seq = unsafe { (*crate::memory::get_retained_block_ptr()).header.sequence };
                state.encoder.encode_reboot_reason(
                    reason as u8,
                    0,
                    boot_seq,
                    0,
                    |chunk| {
                        if upload_ok {
                            if let Err(_) = send(transport, chunk, &mut stats) {
                                upload_ok = false;
                            }
                        }
                    },
                );
                if !upload_ok {
                    return Err(UploadError::EncodingError);
                }
            }

            // 3. FaultRecord (if pending)
            if let Some(fault) = crate::fault::last_fault() {
                state.encoder.encode_fault(&fault, |chunk| {
                    if upload_ok {
                        if let Err(_) = send(transport, chunk, &mut stats) {
                            upload_ok = false;
                        } else {
                            stats.fault_uploaded = true;
                        }
                    }
                });
                if !upload_ok {
                    return Err(UploadError::EncodingError);
                }
            }

            // 4. Metrics
            let metrics_count = state.metrics.len() as u32;
            if metrics_count > 0 {
                state.encoder.encode_metrics(state.metrics.iter(), |chunk| {
                    if upload_ok {
                        if let Err(_) = send(transport, chunk, &mut stats) {
                            upload_ok = false;
                        }
                    }
                });
                if !upload_ok {
                    return Err(UploadError::EncodingError);
                }
                stats.metrics_uploaded = metrics_count;
            }

            // 5. Trace fragments
            let trace_bytes = state.trace.bytes_used() as u32;
            if trace_bytes > 0 {
                state.encoder.encode_trace(&state.trace, |chunk| {
                    if upload_ok {
                        if let Err(_) = send(transport, chunk, &mut stats) {
                            upload_ok = false;
                        }
                    }
                });
                if !upload_ok {
                    return Err(UploadError::EncodingError);
                }
                stats.trace_bytes_uploaded = trace_bytes;
            }

            // 6. Heartbeat
            let uptime = crate::metrics::ticks();
            state.encoder.encode_heartbeat(
                uptime,
                0, // free_stack_bytes — we don't track this
                state.metrics.len() as u32,
                state.trace.frames_lost(),
                |chunk| {
                    if upload_ok {
                        if let Err(_) = send(transport, chunk, &mut stats) {
                            upload_ok = false;
                        }
                    }
                },
            );
            if !upload_ok {
                return Err(UploadError::EncodingError);
            }

            // Success — clear uploaded data
            state.metrics.clear();
            state.trace.clear();
            crate::reboot_reason::clear_reboot_reason();
            crate::fault::clear_fault_record();

            Ok(())
        })?;

        transport.end_session().map_err(UploadError::TransportError)?;

        Ok(stats)
    }
}

/// Async upload for Embassy feature.
#[cfg(feature = "embassy")]
impl UploadManager {
    pub async fn upload_async<T: crate::transport::AsyncChunkTransport>(
        transport: &mut T,
    ) -> Result<UploadStats, UploadError<T::Error>> {
        if !sdk::is_initialized() {
            return Err(UploadError::NotInitialized);
        }

        if !transport.is_available() {
            return Err(UploadError::TransportUnavailable);
        }

        transport.begin_session().await.map_err(UploadError::TransportError)?;

        let mut stats = UploadStats::default();

        // Collect chunks first, then send async
        let mut chunk_buf: heapless::Vec<heapless::Vec<u8, 256>, 32> = heapless::Vec::new();

        sdk::with_sdk(|state| {
            // 1. DeviceInfo
            state.encoder.encode_device_info(
                state.device_id,
                state.firmware_version,
                state.build_id,
                |chunk| {
                    let mut v = heapless::Vec::new();
                    let _ = v.extend_from_slice(chunk);
                    let _ = chunk_buf.push(v);
                },
            );

            // 2. RebootReason
            if let Some(reason) = crate::reboot_reason::last_reboot_reason() {
                let boot_seq = unsafe { (*crate::memory::get_retained_block_ptr()).header.sequence };
                state.encoder.encode_reboot_reason(reason as u8, 0, boot_seq, 0, |chunk| {
                    let mut v = heapless::Vec::new();
                    let _ = v.extend_from_slice(chunk);
                    let _ = chunk_buf.push(v);
                });
            }

            // 3. FaultRecord
            if let Some(fault) = crate::fault::last_fault() {
                state.encoder.encode_fault(&fault, |chunk| {
                    let mut v = heapless::Vec::new();
                    let _ = v.extend_from_slice(chunk);
                    let _ = chunk_buf.push(v);
                });
                stats.fault_uploaded = true;
            }

            // 4. Metrics
            stats.metrics_uploaded = state.metrics.len() as u32;
            state.encoder.encode_metrics(state.metrics.iter(), |chunk| {
                let mut v = heapless::Vec::new();
                let _ = v.extend_from_slice(chunk);
                let _ = chunk_buf.push(v);
            });

            // 5. Trace
            stats.trace_bytes_uploaded = state.trace.bytes_used() as u32;
            state.encoder.encode_trace(&state.trace, |chunk| {
                let mut v = heapless::Vec::new();
                let _ = v.extend_from_slice(chunk);
                let _ = chunk_buf.push(v);
            });

            // 6. Heartbeat
            let uptime = crate::metrics::ticks();
            state.encoder.encode_heartbeat(uptime, 0, state.metrics.len() as u32, state.trace.frames_lost(), |chunk| {
                let mut v = heapless::Vec::new();
                let _ = v.extend_from_slice(chunk);
                let _ = chunk_buf.push(v);
            });
        });

        // Send all chunks
        for chunk in &chunk_buf {
            transport.send_chunk(chunk).await.map_err(UploadError::TransportError)?;
            stats.chunks_sent += 1;
            stats.bytes_sent += chunk.len() as u32;
        }

        // Success — clear buffers
        sdk::with_sdk(|state| {
            state.metrics.clear();
            state.trace.clear();
        });
        crate::reboot_reason::clear_reboot_reason();
        crate::fault::clear_fault_record();

        transport.end_session().await.map_err(UploadError::TransportError)?;

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::ChunkTransport;
    use crate::chunks::decoder::ChunkDecoder;
    use crate::chunks::types::ChunkType;

    extern crate std;
    use std::vec::Vec;

    struct MockTransport {
        sent: Vec<Vec<u8>>,
        available: bool,
        fail_after: Option<usize>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                sent: Vec::new(),
                available: true,
                fail_after: None,
            }
        }

        fn failing_after(n: usize) -> Self {
            Self {
                sent: Vec::new(),
                available: true,
                fail_after: Some(n),
            }
        }
    }

    impl ChunkTransport for MockTransport {
        type Error = &'static str;

        fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
            if let Some(limit) = self.fail_after {
                if self.sent.len() >= limit {
                    return Err("transport error");
                }
            }
            self.sent.push(chunk.to_vec());
            Ok(())
        }

        fn is_available(&self) -> bool {
            self.available
        }
    }

    fn init_sdk_for_test() {
        // Only init if not already initialized
        if !sdk::is_initialized() {
            crate::init(crate::SdkConfig {
                device_id: "test-device",
                firmware_version: "0.1.0",
                build_id: 0x1234,
                ticks_fn: || 0,
                ram_regions: &[],
            });
        }
    }

    #[test]
    fn upload_sends_device_info_first() {
        init_sdk_for_test();

        let mut transport = MockTransport::new();
        let result = UploadManager::upload(&mut transport);
        assert!(result.is_ok());

        // First chunk should be DeviceInfo
        assert!(!transport.sent.is_empty());
        let first = ChunkDecoder::decode(&transport.sent[0]).unwrap();
        assert_eq!(first.chunk_type, ChunkType::DeviceInfo);
    }

    #[test]
    fn upload_sends_heartbeat_last() {
        init_sdk_for_test();

        let mut transport = MockTransport::new();
        let result = UploadManager::upload(&mut transport);
        assert!(result.is_ok());

        let last = ChunkDecoder::decode(transport.sent.last().unwrap()).unwrap();
        assert_eq!(last.chunk_type, ChunkType::Heartbeat);
    }

    #[test]
    fn upload_unavailable_transport() {
        init_sdk_for_test();

        let mut transport = MockTransport::new();
        transport.available = false;
        let result = UploadManager::upload(&mut transport);
        assert!(matches!(result, Err(UploadError::TransportUnavailable)));
    }

    #[test]
    fn upload_clears_buffers_on_success() {
        init_sdk_for_test();

        // Add some metrics
        sdk::with_sdk(|state| {
            let _ = state.metrics.increment("test_key", 1, 0);
        });

        let mut transport = MockTransport::new();
        let result = UploadManager::upload(&mut transport);
        assert!(result.is_ok());

        // Metrics should be cleared
        sdk::with_sdk(|state| {
            assert!(state.metrics.is_empty());
        });
    }
}
