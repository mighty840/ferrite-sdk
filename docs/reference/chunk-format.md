# Chunk Wire Format

This page specifies the binary wire format used by ferrite-sdk to encode all telemetry data. Every piece of data leaving the SDK -- device info, metrics, faults, traces, and heartbeats -- is encoded as one or more chunks.

## Design goals

- **Self-framing:** Each chunk starts with a magic byte (`0xEC`) for stream re-synchronization.
- **Integrity:** CRC-16/CCITT-FALSE covers header + payload. Corrupted chunks are detected.
- **Small:** Maximum chunk size is 256 bytes (8-byte header + 246-byte payload + 2-byte CRC). Fits in a single BLE ATT packet or LoRa frame.
- **Independently decodable:** Each chunk can be decoded without context from previous chunks (except trace fragments, which carry byte offsets for reassembly).

## Chunk structure

Every chunk has the same outer structure:

```
Offset  Size  Field          Description
------  ----  -----          -----------
0       1     magic          0xEC (constant)
1       1     version        Protocol version (currently 1)
2       1     chunk_type     Type identifier (see table)
3       1     flags          Bit flags (see below)
4       2     payload_len    Payload length in bytes (little-endian u16)
6       2     sequence_id    Monotonic sequence number (little-endian u16)
8       N     payload        Type-specific payload (0 to 248 bytes)
8+N     2     crc16          CRC-16/CCITT-FALSE over bytes [0..8+N) (LE u16)
```

**Total size:** 8 (header) + N (payload) + 2 (CRC) = 10 to 258 bytes.

In practice, `MAX_PAYLOAD_SIZE` is 248 bytes, so the maximum total chunk size is 258 bytes. The `MAX_CHUNK_SIZE` constant is set to 256, limiting payload to 246 bytes in some configurations.

### Flags byte

| Bit | Name | Description |
|---|---|---|
| 0 | `LAST` | Set on the final chunk of a multi-chunk sequence |
| 1 | `FRAGMENT` | Reserved for fragmented payloads |
| 2 | `ENCRYPTED` | Payload is AES-128-CCM encrypted |
| 3 | `COMPRESSED` | Payload is RLE compressed |
| 4-7 | -- | Reserved, must be 0 |

### CRC-16/CCITT-FALSE

The CRC is computed over all bytes from offset 0 through offset 7+N (inclusive), i.e., the header and payload but not the CRC itself.

Algorithm: CRC-16/CCITT-FALSE (also known as CRC-16/AUTOSAR or CRC-16/IBM-3740).

- Polynomial: `0x1021`
- Initial value: `0xFFFF`
- Input reflected: No
- Output reflected: No
- Final XOR: `0x0000`

Reference implementation:

```
function crc16_ccitt(data):
    crc = 0xFFFF
    for each byte in data:
        crc = crc XOR (byte << 8)
        for i in 0..8:
            if crc & 0x8000:
                crc = (crc << 1) XOR 0x1021
            else:
                crc = crc << 1
        crc = crc & 0xFFFF
    return crc
```

Test vector: `crc16_ccitt(b"123456789")` = `0x29B1`.

## Chunk types

### 0x01 -- Heartbeat

Sent as the last chunk in every upload session. Provides liveness and diagnostic counters.

```
Offset  Size  Field              Type
------  ----  -----              ----
0       8     uptime_ticks       u64 LE
8       4     free_stack_bytes   u32 LE
12      4     metrics_count      u32 LE
16      4     frames_lost        u32 LE
```

**Total payload: 20 bytes.** Flags: `LAST = 1`.

### 0x02 -- Metrics

Contains a batch of metric entries. Multiple Metrics chunks may be sent if all entries do not fit in one payload.

```
Offset  Size  Field
------  ----  -----
0       1     entry_count     Number of entries in this chunk (u8)
1       ...   entries[]       Packed metric entries (variable)
```

Each metric entry has the format:

```
Offset  Size  Field
------  ----  -----
0       1     key_len         Length of the key string (u8, max 32)
1       K     key             UTF-8 key bytes (K = key_len)
1+K     1     metric_type     0 = Counter, 1 = Gauge, 2 = Histogram
2+K     8     value           Type-dependent value (see below)
10+K    8     timestamp_ticks Monotonic tick count (u64 LE)
```

**Value encoding by metric type:**

| Type | Byte layout |
|---|---|
| Counter (0) | `[counter: u32 LE][padding: 4 bytes zero]` |
| Gauge (1) | `[value: f32 LE][padding: 4 bytes zero]` |
| Histogram (2) | `[min: f32 LE][max: f32 LE]` |

**Entry size:** `1 + key_len + 1 + 8 + 8` = `18 + key_len` bytes.

If all entries fit within `MAX_PAYLOAD_SIZE`, a single Metrics chunk is sent with `LAST = 1`. Otherwise, entries are split across multiple chunks. Intermediate chunks have `LAST = 0`; the final one has `LAST = 1`.

### 0x03 -- FaultRecord

Contains a complete Cortex-M fault dump. Always sent as a single chunk.

```
Offset  Size  Field
------  ----  -----
0       1     fault_type          u8 (0=Hard, 1=Mem, 2=Bus, 3=Usage)
1       32    exception_frame     8 x u32 LE (r0, r1, r2, r3, r12, lr, pc, xpsr)
33      36    extended_regs       9 x u32 LE (r4, r5, r6, r7, r8, r9, r10, r11, sp)
69      64    stack_snapshot      16 x u32 LE (first 16 words above SP)
133     4     cfsr                u32 LE
137     4     hfsr                u32 LE
141     4     mmfar               u32 LE
145     4     bfar                u32 LE
```

**Total payload: 149 bytes.** Flags: `LAST = 1`.

#### Exception frame layout detail

```
Byte offset  Register  Description
-----------  --------  -----------
1            r0        General purpose register 0
5            r1        General purpose register 1
9            r2        General purpose register 2
13           r3        General purpose register 3
17           r12       General purpose register 12
21           lr        Link Register (return address)
25           pc        Program Counter (faulting instruction)
29           xpsr      Program Status Register
```

#### Extended registers layout detail

```
Byte offset  Register  Description
-----------  --------  -----------
33           r4        General purpose register 4
37           r5        General purpose register 5
41           r6        General purpose register 6
45           r7        General purpose register 7
49           r8        General purpose register 8
53           r9        General purpose register 9
57           r10       General purpose register 10
61           r11       General purpose register 11
65           sp        Stack Pointer at fault time
```

### 0x04 -- TraceFragment

Contains a fragment of the trace buffer. Large trace buffers are split into multiple fragments, each prefixed with a byte offset for reassembly.

```
Offset  Size  Field
------  ----  -----
0       8     byte_offset     Starting byte offset in the logical stream (u64 LE)
8       M     data            Raw trace data (M = payload_len - 8)
```

Maximum data per fragment: `MAX_PAYLOAD_SIZE - 8` = 240 bytes.

Multiple TraceFragment chunks may be sent. Intermediate fragments have `LAST = 0`; the final one has `LAST = 1`. The receiver reassembles fragments by concatenating data in byte_offset order.

### 0x05 -- RebootReason

Records why the device rebooted and the boot sequence number.

```
Offset  Size  Field
------  ----  -----
0       1     reason                  u8 (see RebootReason codes)
1       1     extra                   u8 (user-defined, e.g. watchdog ID)
2       4     boot_sequence           u32 LE (incremented each boot)
6       4     uptime_before_reboot    u32 LE (ticks before the reboot)
```

**Total payload: 10 bytes.** Flags: `LAST = 1`.

**Reason codes:**

| Code | Name |
|---|---|
| 0 | Unknown |
| 1 | PowerOnReset |
| 2 | SoftwareReset |
| 3 | WatchdogTimeout |
| 4 | HardFault |
| 5 | MemoryFault |
| 6 | BusFault |
| 7 | UsageFault |
| 8 | AssertFailed |
| 9 | PinReset |
| 10 | BrownoutReset |
| 11 | FirmwareUpdate |
| 12 | UserRequested |

### 0x06 -- DeviceInfo

Identifies the device. Always sent as the first chunk in a session.

```
Offset  Size  Field
------  ----  -----
0       1     device_id_len       u8 (max 127)
1       D     device_id           UTF-8 bytes (D = device_id_len)
1+D     1     fw_version_len      u8 (max 127)
2+D     V     firmware_version    UTF-8 bytes (V = fw_version_len)
2+D+V   8     build_id            u64 LE
```

**Total payload:** `2 + D + V + 8` bytes. Flags: `LAST = 1`.

### 0x07 -- OtaRequest

Sent by the server in response to a heartbeat when an OTA update is available for the device.

```
Offset  Size  Field
------  ----  -----
0       1     version_len         Length of the target version string (u8)
1       V     target_version      UTF-8 version string (V = version_len)
1+V     8     build_id            Target build ID (u64 LE)
```

**Total payload:** `1 + V + 8` bytes. Flags: `LAST = 1`.

### Encrypted chunks

When the `ENCRYPTED` flag (0x04) is set, the payload is wrapped as:

```
Offset  Size  Field
------  ----  -----
0       13    nonce               AES-128-CCM nonce
13      M     ciphertext          Encrypted original payload (M = payload_len - 29)
13+M    16    tag                 Authentication tag
```

The server decrypts the payload using the configured `CHUNK_ENCRYPTION_KEY` before processing.

### Compressed chunks

When the `COMPRESSED` flag (0x08) is set, the payload is RLE-encoded:

```
Offset  Size  Field
------  ----  -----
0       2     original_len        Original uncompressed length (u16 LE)
2       N     rle_data            RLE-compressed data
```

**RLE encoding:** literal bytes are passed through. Runs of 3+ identical bytes are encoded as `0xFF <byte> <count>`. The marker byte `0xFF` itself is encoded as `0xFF 0xFF 0x01`.

## Session structure

A complete upload session consists of chunks sent in this order:

```
[DeviceInfo]                          always, first
[RebootReason]                        if valid record exists
[FaultRecord]                         if valid fault exists
[Metrics] [Metrics] ...               if any metrics buffered (1+ chunks)
[TraceFragment] [TraceFragment] ...   if any trace data buffered (1+ chunks)
[Heartbeat]                           always, last
```

The receiver can use the Heartbeat chunk as a session-complete marker. If a Heartbeat is not received, the session was interrupted (transport error or device reset).

## Sequence numbers

Each chunk carries a 16-bit `sequence_id` that increments monotonically (wrapping at 65535). The sequence is per-encoder-instance (i.e., per device boot). The receiver can use sequence numbers to detect dropped or reordered chunks.

## Decoding example (pseudocode)

```
function decode_chunk(bytes):
    if len(bytes) < 10:
        return error("too short")

    if bytes[0] != 0xEC:
        return error("bad magic")

    if bytes[1] != 1:
        return error("bad version")

    chunk_type = bytes[2]
    flags = bytes[3]
    payload_len = u16_le(bytes[4], bytes[5])
    sequence_id = u16_le(bytes[6], bytes[7])

    total = 8 + payload_len + 2
    if len(bytes) < total:
        return error("truncated")

    expected_crc = u16_le(bytes[8 + payload_len], bytes[9 + payload_len])
    computed_crc = crc16_ccitt(bytes[0 .. 8 + payload_len])

    if expected_crc != computed_crc:
        return error("CRC mismatch")

    payload = bytes[8 .. 8 + payload_len]
    return { chunk_type, flags, sequence_id, payload }
```

## Stream framing

The wire format is designed for byte-oriented streams (UART, TCP, etc.). To decode a stream of concatenated chunks:

1. Scan for the magic byte `0xEC`.
2. Read the next 7 bytes to complete the header.
3. Read `payload_len` bytes of payload.
4. Read 2 bytes of CRC.
5. Validate CRC. If valid, process the chunk. If invalid, skip 1 byte and rescan for magic.

This allows the receiver to recover from partial transmissions or byte-level corruption.
