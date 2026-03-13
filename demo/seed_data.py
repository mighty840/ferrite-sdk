#!/usr/bin/env python3
"""
Ferrite Fleet Demo — Seed Data Generator

Generates realistic telemetry history and POSTs it to ferrite-server.
Run before a demo to pre-populate the dashboard with 24 hours of data.

Usage:
    python3 demo/seed_data.py [--server http://localhost:4000] [--hours 24]

Requires: requests (pip install requests)
"""

import argparse
import math
import random
import struct
import time

import requests

# ── Chunk wire format constants ────────────────────────────────────────

MAGIC = 0xEC
VERSION = 0x01

CHUNK_HEARTBEAT = 0x01
CHUNK_METRICS = 0x02
CHUNK_FAULT = 0x03
CHUNK_REBOOT = 0x05
CHUNK_DEVICE_INFO = 0x06

# CRC-16/CCITT-FALSE
def crc16_ccitt(data: bytes) -> int:
    crc = 0xFFFF
    for byte in data:
        crc ^= byte << 8
        for _ in range(8):
            if crc & 0x8000:
                crc = (crc << 1) ^ 0x1021
            else:
                crc <<= 1
            crc &= 0xFFFF
    return crc


def encode_chunk(chunk_type: int, payload: bytes, seq: int = 0, flags: int = 0) -> bytes:
    """Encode a single wire-format chunk."""
    header = struct.pack("<BBBHH", VERSION, chunk_type, flags, len(payload), seq)
    raw = bytes([MAGIC]) + header
    crc = crc16_ccitt(raw + payload)
    return raw + payload + struct.pack("<H", crc)


def encode_device_info(device_id: str, fw_version: str, build_id: int) -> bytes:
    """Encode a DeviceInfo chunk."""
    did = device_id.encode("utf-8")
    fwv = fw_version.encode("utf-8")
    payload = struct.pack("<B", len(did)) + did
    payload += struct.pack("<B", len(fwv)) + fwv
    payload += struct.pack("<Q", build_id)
    return encode_chunk(CHUNK_DEVICE_INFO, payload)


def encode_metrics(entries: list[tuple[str, int, float, int]], seq: int = 0) -> bytes:
    """
    Encode a Metrics chunk.
    entries: list of (key, metric_type, value, timestamp_ticks)
      metric_type: 0=counter(u32), 1=gauge(f32)
    """
    payload = struct.pack("<B", len(entries))
    for key, mtype, value, ts in entries:
        key_bytes = key.encode("utf-8")
        payload += struct.pack("<B", len(key_bytes)) + key_bytes
        payload += struct.pack("<B", mtype)
        if mtype == 0:  # counter
            payload += struct.pack("<I", int(value))
        else:  # gauge
            payload += struct.pack("<f", value)
        # 4 bytes padding to fill 8-byte value slot
        payload += b"\x00" * 4
        payload += struct.pack("<Q", ts)
    return encode_chunk(CHUNK_METRICS, payload, seq=seq)


def encode_heartbeat(uptime_ticks: int, free_stack: int, metrics_count: int, seq: int = 0) -> bytes:
    """Encode a Heartbeat chunk."""
    payload = struct.pack("<QIII", uptime_ticks, free_stack, metrics_count, 0)
    return encode_chunk(CHUNK_HEARTBEAT, payload, seq=seq)


def encode_reboot(reason: int, boot_seq: int, uptime_before: int) -> bytes:
    """Encode a RebootReason chunk."""
    payload = struct.pack("<BBII", reason, 0, boot_seq, uptime_before)
    return encode_chunk(CHUNK_REBOOT, payload)


def encode_fault(fault_type: int = 0) -> bytes:
    """Encode a FaultRecord chunk with plausible register values."""
    # ExceptionFrame: r0-r3, r12, lr, pc, xpsr (8 x u32)
    pc = random.choice([0x0800_1234, 0x0800_5678, 0x0800_ABCD, 0x0800_2000])
    lr = pc - random.randint(0x10, 0x200)
    frame = struct.pack("<8I",
        random.randint(0, 0xFFFF),  # r0
        random.randint(0, 0xFFFF),  # r1
        0, 0,                        # r2, r3
        0x2000_F000,                 # r12
        lr, pc,
        0x6100_0000,                 # xpsr (Thumb state)
    )
    # Extended: r4-r11, sp (9 x u32)
    extended = struct.pack("<9I",
        *[random.randint(0, 0xFFFF) for _ in range(8)],
        0x2000_FF80,  # sp
    )
    # Stack snapshot: 16 x u32
    stack = struct.pack("<16I", *[random.randint(0, 0xFFFF_FFFF) for _ in range(16)])
    # Fault status: cfsr, hfsr, mmfar, bfar
    cfsr = random.choice([0x0000_0400, 0x0000_0100, 0x0002_0000])  # IMPRECISERR, IBUSERR, INVSTATE
    status = struct.pack("<4I", cfsr, 0x4000_0000, 0, 0)
    payload = struct.pack("<B", fault_type) + frame + extended + stack + status
    return encode_chunk(CHUNK_FAULT, payload)


# ── Fleet devices ──────────────────────────────────────────────────────

FLEET = [
    {
        "device_id": "esp32c3-fleet-01",
        "fw_version": "0.1.0",
        "transport": "WiFi/HTTP",
        "interval_secs": 5,
        "metrics": ["loop_count", "uptime_seconds", "wifi_rssi", "free_heap"],
    },
    {
        "device_id": "nrf5340-fleet-01",
        "fw_version": "0.1.0",
        "transport": "BLE",
        "interval_secs": 30,
        "metrics": ["loop_count", "uptime_seconds"],
    },
    {
        "device_id": "stm32wl55-fleet-01",
        "fw_version": "0.1.0",
        "transport": "LoRa",
        "interval_secs": 60,
        "metrics": ["loop_count", "uptime_seconds", "lora_tx_count", "lora_tx_total"],
    },
    {
        "device_id": "stm32l4a6-fleet-01",
        "fw_version": "0.1.0",
        "transport": "USB CDC",
        "interval_secs": 30,
        "metrics": ["loop_count", "uptime_seconds"],
    },
    {
        "device_id": "stm32h563-fleet-01",
        "fw_version": "0.1.0",
        "transport": "Ethernet",
        "interval_secs": 5,
        "metrics": ["loop_count", "uptime_seconds", "eth_link_up"],
    },
]


def generate_device_history(device: dict, hours: int, build_id: int) -> list[bytes]:
    """Generate a list of chunk batches simulating `hours` of device history."""
    batches = []
    interval = device["interval_secs"]
    total_secs = hours * 3600
    points = total_secs // interval

    # Device info + reboot at start
    boot_batch = encode_device_info(device["device_id"], device["fw_version"], build_id)
    boot_batch += encode_reboot(reason=1, boot_seq=1, uptime_before=0)  # PowerOnReset
    batches.append(boot_batch)

    # Metrics history
    for i in range(1, points + 1):
        t = i * interval
        ticks = t * 1000  # millisecond ticks

        entries = []
        for key in device["metrics"]:
            if key == "loop_count":
                entries.append((key, 0, float(i), ticks))  # counter
            elif key == "uptime_seconds":
                entries.append((key, 1, float(t), ticks))  # gauge
            elif key == "wifi_rssi":
                rssi = -50 + 10 * math.sin(t / 3600 * math.pi)  # varies -40 to -60
                entries.append((key, 1, rssi, ticks))
            elif key == "free_heap":
                heap = 40000 - (i % 500) * 10  # slowly decreasing, resets
                entries.append((key, 1, float(heap), ticks))
            elif key == "lora_tx_count":
                entries.append((key, 0, float(i), ticks))
            elif key == "lora_tx_total":
                entries.append((key, 1, float(i), ticks))
            elif key == "eth_link_up":
                entries.append((key, 1, 1.0, ticks))

        chunk = encode_metrics(entries, seq=i % 65536)

        # Add heartbeat every 10 intervals
        if i % 10 == 0:
            chunk += encode_heartbeat(ticks, 4096, len(entries), seq=i % 65536)

        batches.append(chunk)

    # Inject a fault event ~6 hours in (if enough history)
    if hours >= 6:
        fault_batch = encode_fault(fault_type=0)  # HardFault
        fault_batch += encode_reboot(reason=4, boot_seq=2, uptime_before=6 * 3600)
        fault_batch += encode_device_info(device["device_id"], device["fw_version"], build_id)
        batches.insert(len(batches) // 4, fault_batch)

    return batches


def main():
    parser = argparse.ArgumentParser(description="Seed ferrite-server with demo data")
    parser.add_argument("--server", default="http://localhost:4000", help="Server URL")
    parser.add_argument("--hours", type=int, default=24, help="Hours of history to generate")
    parser.add_argument("--api-key", default=None, help="Ingest API key (X-API-Key header)")
    parser.add_argument("--user", default="admin", help="Basic auth username")
    parser.add_argument("--password", default="admin", help="Basic auth password")
    parser.add_argument("--dry-run", action="store_true", help="Generate but don't POST")
    args = parser.parse_args()

    build_id = int(time.time())
    ingest_url = f"{args.server}/ingest/chunks"

    headers = {"Content-Type": "application/octet-stream"}
    if args.api_key:
        headers["X-API-Key"] = args.api_key
    auth = (args.user, args.password)

    print(f"Seeding {args.hours}h of data for {len(FLEET)} devices → {args.server}")
    print()

    total_chunks = 0
    total_bytes = 0

    for device in FLEET:
        batches = generate_device_history(device, args.hours, build_id)
        device_chunks = len(batches)
        device_bytes = sum(len(b) for b in batches)

        if args.dry_run:
            print(f"  {device['device_id']:25s}  {device_chunks:5d} batches  {device_bytes:8d} bytes  (dry run)")
            total_chunks += device_chunks
            total_bytes += device_bytes
            continue

        # POST in larger batches to avoid thousands of HTTP requests
        BATCH_SIZE = 50
        errors = 0
        for start in range(0, len(batches), BATCH_SIZE):
            combined = b"".join(batches[start:start + BATCH_SIZE])
            try:
                resp = requests.post(ingest_url, data=combined, headers=headers, auth=auth, timeout=10)
                if resp.status_code != 200:
                    errors += 1
                    if errors == 1:
                        print(f"    ERROR: {resp.status_code} {resp.text[:100]}")
            except requests.RequestException as e:
                errors += 1
                if errors == 1:
                    print(f"    ERROR: {e}")

        status = "OK" if errors == 0 else f"{errors} errors"
        print(f"  {device['device_id']:25s}  {device_chunks:5d} batches  {device_bytes:8d} bytes  [{status}]")
        total_chunks += device_chunks
        total_bytes += device_bytes

    print()
    print(f"Total: {total_chunks} batches, {total_bytes:,} bytes")
    if not args.dry_run:
        print(f"Dashboard: {args.server}")


if __name__ == "__main__":
    main()
