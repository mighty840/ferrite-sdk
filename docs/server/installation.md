# Server Installation

## From source

```bash
# Clone the repository
git clone https://github.com/mighty840/ferrite-sdk.git
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

The image `ghcr.io/mighty840/ferrite` bundles the server and the dashboard. It's built from the repository's `Dockerfile`.

```bash
docker run -p 4000:4000 -v ferrite-data:/home/ferrite/data \
  -e BASIC_AUTH_USER=admin -e BASIC_AUTH_PASS=change-me \
  ghcr.io/mighty840/ferrite:latest
```

Everything the server writes lives under `/home/ferrite/data`, so mount a volume there:

| Path | Contents |
|---|---|
| `ferrite.db` | Devices, faults, metrics, OTA targets, campaigns |
| `firmware/` | Uploaded OTA images |
| `elfs/` | Uploaded ELF files for symbolication |

Configure the server with the [environment variables](/server/configuration#environment-variables). One variable belongs to the image itself:

| Variable | Default | Description |
|---|---|---|
| `DB_RESET_INTERVAL` | `0` | **Demo only.** Every N seconds, stop the server, delete `ferrite.db`, `firmware/` and `elfs/`, and start again. `0` disables it |

## Verify

```bash
# Start the server (Basic auth needs a password)
BASIC_AUTH_PASS=change-me ferrite-server --http 0.0.0.0:4000

# In another terminal, check it answers
curl -u admin:change-me http://localhost:4000/devices
# Should return: {"devices":[]}
```
