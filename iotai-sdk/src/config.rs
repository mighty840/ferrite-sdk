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
