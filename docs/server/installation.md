# Server Installation

## From source

```bash
# Clone the repository
git clone https://github.com/your-org/ferrite-sdk.git
cd ferrite-sdk

# Build the server
cargo build -p ferrite-server --release

# The binary is at:
# target/release/ferrite-server
```

### Dependencies

The server requires:

- Rust 1.75+ (stable)
- SQLite 3 development libraries (usually pre-installed; on Debian/Ubuntu: `apt install libsqlite3-dev`)
- (Optional) `arm-none-eabi-addr2line` for fault symbolication

### Install addr2line

For symbolication to work, install the ARM toolchain:

```bash
# Ubuntu/Debian
sudo apt install gcc-arm-none-eabi

# macOS (Homebrew)
brew install arm-none-eabi-gcc

# Or download from ARM:
# https://developer.arm.com/downloads/-/gnu-rm
```

The server auto-detects `arm-none-eabi-addr2line` on your PATH. You can also specify the path explicitly with `--addr2line /path/to/arm-none-eabi-addr2line`.

## Docker

```dockerfile
FROM rust:1.80 AS builder
WORKDIR /src
COPY . .
RUN cargo build -p ferrite-server --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libsqlite3-0 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/ferrite-server /usr/local/bin/
EXPOSE 4000
ENTRYPOINT ["ferrite-server", "--http", "0.0.0.0:4000"]
```

```bash
docker build -t ferrite-server .
docker run -p 4000:4000 -v $(pwd)/data:/data ferrite-server --db /data/ferrite.db --elf-dir /data/elfs
```

## Verify

```bash
# Start the server
ferrite-server --http 0.0.0.0:4000

# In another terminal, check health
curl http://localhost:4000/devices
# Should return: {"devices":[]}
```
