#!/usr/bin/env bash
#
# Ferrite RPi Gateway — Setup Script
#
# Prepares a Raspberry Pi as a ferrite edge gateway running:
#   - ferrite-server  (port 4000, SQLite, dashboard)
#   - ferrite-gateway (BLE scanner + USB serial bridge)
#
# Usage:
#   scp -r deploy/rpi-gateway/ pi@raspberrypi:~/ferrite-deploy/
#   ssh pi@raspberrypi
#   cd ~/ferrite-deploy && sudo bash setup.sh
#
# Prerequisites:
#   - Raspberry Pi OS (64-bit recommended for BLE stack)
#   - Rust toolchain on build host (cross-compile, then copy binaries)
#   - BlueZ installed for BLE support

set -euo pipefail

INSTALL_DIR="/opt/ferrite"
SERVICE_USER="ferrite"

echo "=== Ferrite RPi Gateway Setup ==="

# ── 1. Create service user ────────────────────────────────────────────
if ! id "$SERVICE_USER" &>/dev/null; then
    echo "[1/6] Creating service user: $SERVICE_USER"
    sudo useradd --system --shell /usr/sbin/nologin --home-dir "$INSTALL_DIR" "$SERVICE_USER"
else
    echo "[1/6] Service user $SERVICE_USER already exists"
fi

# ── 2. Create directory structure ──────────────────────────────────────
echo "[2/6] Creating directories"
sudo mkdir -p "$INSTALL_DIR"/{bin,data,data/elfs,env}
sudo chown -R "$SERVICE_USER:$SERVICE_USER" "$INSTALL_DIR"

# ── 3. Copy environment files ─────────────────────────────────────────
echo "[3/6] Installing environment files"
sudo cp server.env "$INSTALL_DIR/env/server.env"
sudo cp gateway.env "$INSTALL_DIR/env/gateway.env"
sudo chmod 600 "$INSTALL_DIR/env/"*.env
sudo chown "$SERVICE_USER:$SERVICE_USER" "$INSTALL_DIR/env/"*.env

# ── 4. Install binaries ───────────────────────────────────────────────
echo "[4/6] Installing binaries"
if [ -f ferrite-server ] && [ -f ferrite-gateway ]; then
    sudo cp ferrite-server ferrite-gateway "$INSTALL_DIR/bin/"
    sudo chmod 755 "$INSTALL_DIR/bin/"*
    sudo chown "$SERVICE_USER:$SERVICE_USER" "$INSTALL_DIR/bin/"*
else
    echo "  WARNING: binaries not found in current directory."
    echo "  Cross-compile and copy them here before running setup:"
    echo ""
    echo "    # On build host:"
    echo "    cross build --release -p ferrite-server --target aarch64-unknown-linux-gnu"
    echo "    cross build --release -p ferrite-gateway --target aarch64-unknown-linux-gnu"
    echo "    scp target/aarch64-unknown-linux-gnu/release/{ferrite-server,ferrite-gateway} pi@raspberrypi:~/ferrite-deploy/"
    echo ""
fi

# ── 5. Install systemd services ───────────────────────────────────────
echo "[5/6] Installing systemd services"
sudo cp ferrite-server.service /etc/systemd/system/
sudo cp ferrite-gateway.service /etc/systemd/system/
sudo systemctl daemon-reload

# ── 6. Enable and start ───────────────────────────────────────────────
echo "[6/6] Enabling services"
sudo systemctl enable ferrite-server ferrite-gateway

# Add ferrite user to required groups for BLE and USB access
sudo usermod -aG dialout "$SERVICE_USER" 2>/dev/null || true
sudo usermod -aG bluetooth "$SERVICE_USER" 2>/dev/null || true

echo ""
echo "=== Setup complete ==="
echo ""
echo "Next steps:"
echo "  1. Edit credentials:  sudo nano $INSTALL_DIR/env/server.env"
echo "  2. Edit gateway config: sudo nano $INSTALL_DIR/env/gateway.env"
echo "  3. Copy cross-compiled binaries to $INSTALL_DIR/bin/ (if not done)"
echo "  4. Start services:    sudo systemctl start ferrite-server ferrite-gateway"
echo "  5. Check status:      sudo systemctl status ferrite-server ferrite-gateway"
echo "  6. View logs:         journalctl -u ferrite-server -u ferrite-gateway -f"
echo ""
echo "Dashboard will be available at: http://$(hostname -I | awk '{print $1}'):4000"
