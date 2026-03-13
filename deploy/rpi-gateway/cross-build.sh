#!/usr/bin/env bash
#
# Cross-compile ferrite-server and ferrite-gateway for Raspberry Pi (aarch64).
#
# Prerequisites:
#   cargo install cross
#
# Usage:
#   cd /path/to/ferrite-sdk
#   bash deploy/rpi-gateway/cross-build.sh
#
# Output binaries land in target/aarch64-unknown-linux-gnu/release/

set -euo pipefail

TARGET="aarch64-unknown-linux-gnu"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$PROJECT_ROOT"

echo "=== Cross-compiling for $TARGET ==="

echo "[1/2] Building ferrite-server..."
cross build --release -p ferrite-server --target "$TARGET"

echo "[2/2] Building ferrite-gateway..."
cross build --release -p ferrite-gateway --target "$TARGET"

RELEASE_DIR="target/$TARGET/release"

echo ""
echo "=== Build complete ==="
echo "Binaries:"
ls -lh "$RELEASE_DIR/ferrite-server" "$RELEASE_DIR/ferrite-gateway"
echo ""
echo "Deploy to RPi:"
echo "  scp $RELEASE_DIR/{ferrite-server,ferrite-gateway} pi@raspberrypi:~/ferrite-deploy/"
echo "  scp -r deploy/rpi-gateway/ pi@raspberrypi:~/ferrite-deploy/"
echo "  ssh pi@raspberrypi 'cd ~/ferrite-deploy && sudo bash setup.sh'"
