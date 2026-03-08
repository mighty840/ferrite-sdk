# Contributing

Thank you for your interest in contributing to ferrite-sdk. This guide covers the development workflow, code conventions, and how to submit changes.

## Development setup

```bash
# Clone the repository
git clone https://github.com/mighty840/ferrite-sdk.git
cd ferrite-sdk

# Install the ARM target for embedded builds
rustup target add thumbv7em-none-eabihf

# Run host tests (no embedded toolchain needed)
cargo build -p ferrite-sdk --no-default-features
cargo test -p ferrite-sdk --no-default-features
cargo test -p ferrite-server
```

## Project conventions

- **No alloc** in the `ferrite-sdk` core crate. All buffers are fixed-size.
- **No panics** in production code paths. Functions return `Result<T, SdkError>` or silently handle errors. Tests may panic.
- **Feature flags** gate all hardware dependencies. The `cortex-m` feature must be disabled for host tests.
- **`critical-section`** crate provides portable critical sections across embedded and host targets.
- **Hand-rolled encoding** for the chunk wire format. Keep it simple and dependency-free.

## Workspace structure

The workspace contains five crates:

| Crate | Tests run on |
|---|---|
| `ferrite-sdk` | Host (`--no-default-features`) |
| `ferrite-embassy` | Not tested on host (requires Embassy runtime) |
| `ferrite-rtic` | Not tested on host (requires RTIC runtime) |
| `ferrite-ffi` | Not tested on host (produces staticlib) |
| `ferrite-server` | Host |

## Adding a new ChunkType

1. Add the variant to the `ChunkType` enum in `ferrite-sdk/src/chunks/types.rs`
2. Add an `encode_*` method to `ChunkEncoder` in `chunks/encoder.rs`
3. Add a decode match arm in `ChunkDecoder` in `chunks/decoder.rs`
4. Add a SQL column or table in `ferrite-server/src/store.rs`
5. Add a parser and handler in `ferrite-server/src/ingest.rs`
6. Add tests for encode/decode roundtrip

## Pull request checklist

- [ ] `cargo test -p ferrite-sdk --no-default-features` passes
- [ ] `cargo test -p ferrite-server` passes
- [ ] `cargo build -p ferrite-sdk --features cortex-m,defmt,embassy --target thumbv7em-none-eabihf` succeeds
- [ ] No new `alloc` usage in `ferrite-sdk`
- [ ] No new panics in production code paths
- [ ] New public API items have doc comments
- [ ] Wire format changes are documented in `reference/chunk-format.md`

## License

Contributions are accepted under the MIT OR Apache-2.0 dual license.
