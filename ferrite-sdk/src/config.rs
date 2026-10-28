// Compile-time configuration constants for the SDK.
// Users can override buffer sizes via const generics on the SDK types.

/// Default number of metric entries in the metrics buffer.
pub const DEFAULT_METRICS_CAPACITY: usize = 32;

/// Default size of the trace buffer in bytes.
pub const DEFAULT_TRACE_BUFFER_SIZE: usize = 512;

/// Maximum chunk size in bytes (header + payload + CRC).
pub const MAX_CHUNK_SIZE: usize = 256;

/// Maximum payload size per chunk.
pub const MAX_PAYLOAD_SIZE: usize = 248;

/// Largest payload the encoder puts in one chunk: what fits in
/// [`MAX_CHUNK_SIZE`] after the header and CRC (10 bytes). With the
/// `encryption` feature it also leaves room for the nonce and tag (21 bytes),
/// so every chunk the SDK produces can be sealed.
pub const MAX_PLAIN_PAYLOAD_SIZE: usize = if cfg!(feature = "encryption") {
    MAX_CHUNK_SIZE - 10 - 21
} else {
    MAX_CHUNK_SIZE - 10
};
