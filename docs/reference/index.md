# API Reference

This section documents the public API surface of iotai-sdk across all language bindings and the wire format specification.

## Reference pages

- [Rust SDK API](./sdk-api) -- All public types, functions, traits, and macros in the `iotai-sdk` crate
- [C FFI API](./c-api) -- Every `extern "C"` function, typedef, and struct in `iotai-sdk-ffi`
- [Chunk Wire Format](./chunk-format) -- Byte-level specification of the binary chunk protocol
- [SdkConfig](./config) -- All configuration fields and compile-time constants

## Quick reference

### Common Rust imports

```rust
use iotai_sdk::{SdkConfig, RamRegion, RebootReason, SdkError};
use iotai_sdk::transport::{ChunkTransport, AsyncChunkTransport};
use iotai_sdk::upload::{UploadManager, UploadStats, UploadError};
use iotai_sdk::fault::{FaultRecord, FaultType};
use iotai_sdk::metrics::{MetricsBuffer, MetricValue, MetricType};
use iotai_sdk::trace::TraceBuffer;
use iotai_sdk::chunks::types::{ChunkType, ChunkHeader, DecodedChunk};
use iotai_sdk::chunks::encoder::ChunkEncoder;
use iotai_sdk::chunks::decoder::ChunkDecoder;
```

### Common C includes

```c
#include "iotai_sdk.h"

// All types are prefixed with iotai_ or IOTAI_
// All functions are prefixed with iotai_
```
