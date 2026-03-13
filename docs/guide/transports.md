# Transport Layer

ferrite-sdk is transport-agnostic. All telemetry flows through the `ChunkTransport` trait (or its async counterpart `AsyncChunkTransport`), and the SDK ships with implementations for UART, USB CDC, BLE, HTTP, and LoRa.

## Transport traits

```rust
pub trait ChunkTransport {
    type Error: core::fmt::Debug;
    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error>;
    fn is_available(&self) -> bool;
    fn begin_session(&mut self) -> Result<(), Self::Error>;
    fn end_session(&mut self) -> Result<(), Self::Error>;
}

pub trait AsyncChunkTransport {
    type Error: core::fmt::Debug;
    async fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error>;
    async fn is_available(&self) -> bool;
    async fn begin_session(&mut self) -> Result<(), Self::Error>;
    async fn end_session(&mut self) -> Result<(), Self::Error>;
}
```

## Built-in transports

### UART (always available)

The default transport. Works with any `embedded_io::Write` UART peripheral.

```rust
use ferrite_sdk::transport::UartTransport;

let transport = UartTransport::new(uart_peripheral);
```

### USB CDC (`usb-cdc` feature)

Uses `embassy-usb` `CdcAcmClass` for USB serial communication. Waits for a host connection before sending.

```toml
[dependencies]
ferrite-sdk = { version = "0.1", features = ["usb-cdc"] }
```

### HTTP (`http` feature)

WiFi/HTTP transport using `reqwless` (no_std HTTP client). Posts chunks to `/ingest/chunks` with optional API key authentication.

```toml
[dependencies]
ferrite-sdk = { version = "0.1", features = ["http"] }
```

```rust
use ferrite_sdk::transport::http::HttpTransport;

let transport = HttpTransport::new(
    tcp_connection,
    "http://ferrite.local:4000",
    Some("my-api-key"),
);
```

### LoRa (`lora` feature)

SX1262/SX1276 radio transport via `embedded-hal` SPI. Sends each chunk as a single LoRa packet.

```toml
[dependencies]
ferrite-sdk = { version = "0.1", features = ["lora"] }
```

```rust
use ferrite_sdk::transport::lora::{LoraTransport, LoraConfig};

let config = LoraConfig {
    frequency: 915_000_000,  // US ISM band
    spreading_factor: 7,
    tx_power: 14,
    ..Default::default()
};
let transport = LoraTransport::new(spi_device, config);
```

**Maximum payload by spreading factor:**

| SF | Max Payload |
|---|---|
| 7-8 | 222 bytes |
| 9 | 115 bytes |
| 10-12 | 51 bytes |

### BLE (separate crate)

BLE transport is in the [`ferrite-ble-nrf`](https://github.com/mighty840/ferrite-sdk/tree/main/ferrite-ble-nrf) crate (excluded from workspace, requires nRF SoftDevice). Defines a custom GATT service:

- **Service UUID:** `FE771E00-0001-4000-8000-00805F9B34FB`
- **Chunk characteristic:** `FE771E00-0002-4000-8000-00805F9B34FB`
- **Max BLE payload:** 244 bytes (BLE 5.0 MTU)

## Compression wrapper

The `CompressedTransport<T>` wraps any transport to apply RLE compression before sending. Useful for bandwidth-constrained links (LoRa, BLE).

```rust
use ferrite_sdk::compression::CompressedTransport;

let compressed = CompressedTransport::new(lora_transport);
// Chunks are RLE-compressed transparently
```

Compressed chunks set the `FLAG_COMPRESSED` (0x08) flag in the header. The server decompresses them automatically.

## Edge gateway

For devices that can't reach the server directly, the [`ferrite-gateway`](../gateway/) daemon bridges BLE, USB, and LoRa devices to the server over HTTP. It runs on a Raspberry Pi or similar edge device and provides offline buffering with automatic retry.
