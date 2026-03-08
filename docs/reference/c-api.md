# C FFI API Reference

This page documents every function, struct, and typedef exported by the `iotai-sdk-ffi` crate. The C header file is `iotai_sdk.h`.

## Error codes

```c
typedef enum {
    IOTAI_OK              =  0,   // Success
    IOTAI_NOT_INITIALIZED = -1,   // iotai_sdk_init() was not called
    IOTAI_ALREADY_INIT    = -2,   // iotai_sdk_init() called more than once
    IOTAI_BUFFER_FULL     = -3,   // Metric buffer overflow (oldest evicted)
    IOTAI_KEY_TOO_LONG    = -4,   // Metric key exceeds 32 characters
    IOTAI_NULL_PTR        = -5,   // A required pointer argument was NULL
    IOTAI_ENCODING        = -6,   // Chunk encoding failed
    IOTAI_TRANSPORT       = -7,   // Transport callback returned an error
} iotai_error_t;
```

All functions return `iotai_error_t`. Check the return value after every call.

---

## Initialization

### `iotai_sdk_init`

```c
iotai_error_t iotai_sdk_init(
    const char                *device_id,
    const char                *firmware_version,
    uint64_t                   build_id,
    uint64_t                 (*ticks_fn)(void),
    const iotai_ram_region_t  *ram_regions,
    uint32_t                   ram_region_count
);
```

Initialize the SDK. Call exactly once at firmware startup, before calling any other `iotai_*` function.

**Parameters:**

| Parameter | Type | Description |
|---|---|---|
| `device_id` | `const char*` | NUL-terminated device identifier. Must point to memory that lives for the entire program (typically a string literal). |
| `firmware_version` | `const char*` | NUL-terminated firmware version string. Same lifetime requirement. |
| `build_id` | `uint64_t` | First 8 bytes of the ELF `.build_id` section. Pass 0 if not available. |
| `ticks_fn` | `uint64_t (*)(void)` | Function returning the current monotonic tick count. Must not be NULL. |
| `ram_regions` | `const iotai_ram_region_t*` | Array of valid RAM address ranges. May be NULL if `ram_region_count` is 0. |
| `ram_region_count` | `uint32_t` | Number of elements in the `ram_regions` array. Maximum 4. |

**Returns:** `IOTAI_OK` on success, `IOTAI_NULL_PTR` if a required pointer is NULL, `IOTAI_ALREADY_INIT` if called more than once.

**Example:**

```c
static uint64_t get_ticks(void) {
    return (uint64_t)k_uptime_ticks();
}

iotai_ram_region_t regions[] = {
    { .start = 0x20000000, .end = 0x20040000 },
};

iotai_error_t err = iotai_sdk_init(
    "my-device-01",
    "1.2.3",
    0,
    get_ticks,
    regions,
    1
);
```

---

## Reboot reason

### `iotai_record_reboot_reason`

```c
iotai_error_t iotai_record_reboot_reason(uint8_t reason);
```

Record the reboot reason for the current boot cycle. Call after reading the MCU's reset-cause register.

**Parameters:**

| Parameter | Type | Description |
|---|---|---|
| `reason` | `uint8_t` | Reason code (see table below) |

**Reason codes:**

| Code | Name | Description |
|---|---|---|
| 0 | Unknown | Default / unrecognized |
| 1 | PowerOnReset | Initial power-on |
| 2 | SoftwareReset | `AIRCR.SYSRESETREQ` |
| 3 | WatchdogTimeout | Watchdog timer expired |
| 4 | HardFault | Cortex-M HardFault (set automatically by fault handler) |
| 5 | MemoryFault | MPU violation |
| 6 | BusFault | Bus error |
| 7 | UsageFault | Undefined instruction, etc. |
| 8 | AssertFailed | Application assert |
| 9 | PinReset | External reset pin |
| 10 | BrownoutReset | Supply voltage dropped |
| 11 | FirmwareUpdate | Pre-update reset |
| 12 | UserRequested | Application-initiated reset |

**Returns:** `IOTAI_OK` or `IOTAI_NOT_INITIALIZED`.

### `iotai_last_reboot_reason`

```c
iotai_error_t iotai_last_reboot_reason(uint8_t *out_reason);
```

Retrieve the reboot reason recorded by the previous boot. If no valid record exists, writes 0 (Unknown) to `*out_reason`.

**Parameters:**

| Parameter | Type | Description |
|---|---|---|
| `out_reason` | `uint8_t*` | Output pointer for the reason code. Must not be NULL. |

**Returns:** `IOTAI_OK` or `IOTAI_NULL_PTR`.

---

## Metrics

### `iotai_metric_increment`

```c
iotai_error_t iotai_metric_increment(const char *key, uint32_t delta);
```

Increment a counter metric. If the key does not exist, a new entry is created with value `delta`. If it exists, `delta` is added to the current value (wrapping at `UINT32_MAX`).

**Parameters:**

| Parameter | Type | Description |
|---|---|---|
| `key` | `const char*` | NUL-terminated metric key, max 32 characters. Must not be NULL. |
| `delta` | `uint32_t` | Value to add to the counter. |

**Returns:** `IOTAI_OK`, `IOTAI_NOT_INITIALIZED`, `IOTAI_NULL_PTR`, or `IOTAI_KEY_TOO_LONG`.

### `iotai_metric_gauge`

```c
iotai_error_t iotai_metric_gauge(const char *key, float value);
```

Set a gauge metric. Overwrites any previous value for this key.

**Parameters:**

| Parameter | Type | Description |
|---|---|---|
| `key` | `const char*` | NUL-terminated metric key, max 32 characters. |
| `value` | `float` | New gauge value. |

**Returns:** `IOTAI_OK`, `IOTAI_NOT_INITIALIZED`, `IOTAI_NULL_PTR`, or `IOTAI_KEY_TOO_LONG`.

### `iotai_metric_observe`

```c
iotai_error_t iotai_metric_observe(const char *key, float value);
```

Record a histogram observation. Updates min, max, sum, and count for the given key.

**Parameters:**

| Parameter | Type | Description |
|---|---|---|
| `key` | `const char*` | NUL-terminated metric key, max 32 characters. |
| `value` | `float` | Observed value. |

**Returns:** `IOTAI_OK`, `IOTAI_NOT_INITIALIZED`, `IOTAI_NULL_PTR`, or `IOTAI_KEY_TOO_LONG`.

---

## Fault record

### `iotai_last_fault`

```c
iotai_error_t iotai_last_fault(iotai_fault_record_t *out);
```

Retrieve the fault record from the previous boot. If no valid fault is present, `out->valid` is set to `false`.

**Parameters:**

| Parameter | Type | Description |
|---|---|---|
| `out` | `iotai_fault_record_t*` | Output pointer for the fault record. Must not be NULL. |

**Returns:** `IOTAI_OK` or `IOTAI_NULL_PTR`.

### `iotai_fault_record_t`

```c
typedef struct {
    bool     valid;           // true if a fault was captured
    uint8_t  fault_type;      // 0=HardFault, 1=MemFault, 2=BusFault, 3=UsageFault
    uint8_t  _pad[2];

    // Hardware exception frame (pushed by CPU on fault entry)
    uint32_t r0, r1, r2, r3;
    uint32_t r12, lr, pc, xpsr;

    // Software-captured registers
    uint32_t r4, r5, r6, r7, r8, r9, r10, r11;
    uint32_t sp;

    // Stack snapshot: first 16 words above SP at fault time
    uint32_t stack_snapshot[16];

    // Cortex-M fault status registers
    uint32_t cfsr;    // Configurable Fault Status Register
    uint32_t hfsr;    // HardFault Status Register
    uint32_t mmfar;   // MemManage Fault Address Register
    uint32_t bfar;    // BusFault Address Register
} iotai_fault_record_t;
```

**Usage:**

```c
iotai_fault_record_t fault;
iotai_last_fault(&fault);

if (fault.valid) {
    printf("Fault type=%u at PC=0x%08X, LR=0x%08X\n",
           fault.fault_type, fault.pc, fault.lr);
    printf("CFSR=0x%08X HFSR=0x%08X\n", fault.cfsr, fault.hfsr);

    // Print stack snapshot
    for (int i = 0; i < 16; i++) {
        printf("  SP+%02d: 0x%08X\n", i * 4, fault.stack_snapshot[i]);
    }
}
```

---

## Upload

### `iotai_upload`

```c
iotai_error_t iotai_upload(
    const iotai_transport_t  *transport,
    iotai_upload_stats_t     *out_stats
);
```

Run a full blocking upload session. Sends all pending data (DeviceInfo, RebootReason, FaultRecord, Metrics, Trace, Heartbeat) through the transport callbacks.

On success, clears all uploaded data from the SDK buffers. On transport error, data is retained for the next attempt.

**Parameters:**

| Parameter | Type | Description |
|---|---|---|
| `transport` | `const iotai_transport_t*` | Transport descriptor. Must not be NULL. `send_chunk` must not be NULL. |
| `out_stats` | `iotai_upload_stats_t*` | Optional output for upload statistics. May be NULL. |

**Returns:** `IOTAI_OK`, `IOTAI_NOT_INITIALIZED`, `IOTAI_NULL_PTR`, `IOTAI_TRANSPORT`, or `IOTAI_ENCODING`.

### `iotai_transport_t`

```c
typedef int32_t (*iotai_send_chunk_fn)(
    const uint8_t *data,
    uint32_t       len,
    void          *ctx
);

typedef bool (*iotai_is_available_fn)(void *ctx);

typedef struct {
    iotai_send_chunk_fn   send_chunk;    // Required
    iotai_is_available_fn is_available;  // Optional (NULL = always available)
    void                 *ctx;           // Opaque context forwarded to callbacks
} iotai_transport_t;
```

The `send_chunk` callback must return 0 on success, non-zero on error. If it returns non-zero, the upload session is aborted and `iotai_upload()` returns `IOTAI_TRANSPORT`.

The `ctx` pointer is forwarded to every callback invocation. Use it to pass peripheral handles, socket descriptors, or other transport state without global variables.

### `iotai_upload_stats_t`

```c
typedef struct {
    uint32_t chunks_sent;
    uint32_t bytes_sent;
    bool     fault_uploaded;
    uint32_t metrics_uploaded;
    uint32_t trace_bytes_uploaded;
} iotai_upload_stats_t;
```

---

## Structs

### `iotai_ram_region_t`

```c
typedef struct {
    uint32_t start;  // Inclusive start address
    uint32_t end;    // Exclusive end address
} iotai_ram_region_t;
```

Defines a valid RAM address range for the fault handler's stack snapshot. The fault handler will only read addresses that fall within a registered region, preventing secondary faults from accessing invalid memory.

---

## Thread safety summary

| Function | ISR-safe | Thread-safe | Reentrant |
|---|---|---|---|
| `iotai_sdk_init` | No | No | No |
| `iotai_record_reboot_reason` | Yes | Yes | Yes |
| `iotai_last_reboot_reason` | Yes | Yes | Yes |
| `iotai_metric_increment` | Yes | Yes | Yes |
| `iotai_metric_gauge` | Yes | Yes | Yes |
| `iotai_metric_observe` | Yes | Yes | Yes |
| `iotai_last_fault` | Yes | Yes | Yes |
| `iotai_upload` | No | Yes (single caller) | No |

"ISR-safe" means the function can be called from an interrupt service routine. All metric functions use Cortex-M critical sections (interrupt disable/enable) and are safe from any context.

`iotai_upload` must not be called from an ISR because the transport callback may block. It should be called from exactly one thread/task at a time.
