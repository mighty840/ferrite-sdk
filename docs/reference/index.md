# API Reference

This section documents the public API surface of ferrite-sdk across all language bindings and the wire format specification.

## Reference pages

- [Rust SDK API](./sdk-api) -- All public types, functions, traits, and macros in the `ferrite-sdk` crate
- [C FFI API](./c-api) -- Every `extern "C"` function, typedef, and struct in `ferrite-ffi`
- [Chunk Wire Format](./chunk-format) -- Byte-level specification of the binary chunk protocol
- [SdkConfig](./config) -- All configuration fields and compile-time constants

## Quick reference

### Common Rust imports

```rust
use ferrite_sdk::{SdkConfig, RamRegion, RebootReason, SdkError};
use ferrite_sdk::transport::{ChunkTransport, AsyncChunkTransport};
use ferrite_sdk::upload::{UploadManager, UploadStats, UploadError};
use ferrite_sdk::fault::{FaultRecord, FaultType};
use ferrite_sdk::metrics::{MetricsBuffer, MetricValue, MetricType};
use ferrite_sdk::trace::TraceBuffer;
use ferrite_sdk::chunks::types::{ChunkType, ChunkHeader, DecodedChunk};
use ferrite_sdk::chunks::encoder::ChunkEncoder;
use ferrite_sdk::chunks::decoder::ChunkDecoder;
```

### Common C includes

```c
#include "ferrite_sdk.h"

// All types are prefixed with ferrite_ or FERRITE_
// All functions are prefixed with ferrite_
```
