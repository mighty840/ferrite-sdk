#!/usr/bin/env bash
#
# Ferrite Fleet Demo — Pre-flight Check
#
# Run this on the RPi before a demo to verify everything is ready.
#
# Usage:
#   bash demo/preflight.sh [server-url]
#
# Example:
#   bash demo/preflight.sh http://localhost:4000

set -uo pipefail

SERVER="${1:-http://localhost:4000}"
PASS=0
FAIL=0
WARN=0

pass() { echo "  ✓ $1"; ((PASS++)); }
fail() { echo "  ✗ $1"; ((FAIL++)); }
warn() { echo "  ? $1"; ((WARN++)); }

echo "=== Ferrite Fleet Demo — Pre-flight Check ==="
echo "Server: $SERVER"
echo ""

# ── Services ───────────────────────────────────────────────────────────
echo "Services:"

if systemctl is-active --quiet ferrite-server 2>/dev/null; then
    pass "ferrite-server is running"
else
    fail "ferrite-server is NOT running (sudo systemctl start ferrite-server)"
fi

if systemctl is-active --quiet ferrite-gateway 2>/dev/null; then
    pass "ferrite-gateway is running"
else
    fail "ferrite-gateway is NOT running (sudo systemctl start ferrite-gateway)"
fi

echo ""

# ── Server Health ──────────────────────────────────────────────────────
echo "Server:"

HEALTH=$(curl -sf "$SERVER/health" 2>/dev/null)
if [ $? -eq 0 ]; then
    pass "Health endpoint OK"
else
    fail "Health endpoint unreachable at $SERVER/health"
fi

AUTH_MODE=$(curl -sf "$SERVER/auth/mode" 2>/dev/null | grep -o '"mode":"[^"]*"' | head -1)
if [ -n "$AUTH_MODE" ]; then
    pass "Auth mode: $AUTH_MODE"
else
    warn "Could not detect auth mode"
fi

echo ""

# ── USB Transport ──────────────────────────────────────────────────────
echo "USB Transport:"

if ls /dev/ttyACM* &>/dev/null; then
    for port in /dev/ttyACM*; do
        pass "USB device found: $port"
    done
else
    warn "No USB ACM devices found (plug in Nucleo board)"
fi

echo ""

# ── BLE Transport ──────────────────────────────────────────────────────
echo "BLE Transport:"

if command -v bluetoothctl &>/dev/null; then
    if bluetoothctl show 2>/dev/null | grep -q "Powered: yes"; then
        pass "Bluetooth adapter powered on"
    else
        fail "Bluetooth adapter is off (sudo bluetoothctl power on)"
    fi
else
    warn "bluetoothctl not found — install bluez for BLE support"
fi

echo ""

# ── Network ────────────────────────────────────────────────────────────
echo "Network:"

IP=$(hostname -I 2>/dev/null | awk '{print $1}')
if [ -n "$IP" ]; then
    pass "RPi IP: $IP"
    pass "Dashboard URL: http://$IP:4000"
else
    fail "No IP address detected"
fi

echo ""

# ── Registered Devices ─────────────────────────────────────────────────
echo "Devices:"

DEVICES=$(curl -sf -u admin:changeme "$SERVER/devices" 2>/dev/null)
if [ $? -eq 0 ]; then
    COUNT=$(echo "$DEVICES" | grep -o '"device_key"' | wc -l)
    if [ "$COUNT" -gt 0 ]; then
        pass "$COUNT device(s) registered"
        echo "$DEVICES" | grep -o '"device_key":"[^"]*"' | while read -r line; do
            echo "       $(echo "$line" | sed 's/"device_key":"/  /;s/"//')"
        done
    else
        warn "No devices registered yet (waiting for first data)"
    fi
else
    warn "Could not query devices (auth may need adjustment)"
fi

echo ""

# ── Summary ────────────────────────────────────────────────────────────
echo "─────────────────────────────────"
echo "Results: $PASS passed, $FAIL failed, $WARN warnings"

if [ "$FAIL" -gt 0 ]; then
    echo ""
    echo "Fix the failures above before starting the demo."
    exit 1
else
    echo ""
    echo "Ready for demo!"
    exit 0
fi
