# Zephyr (C) Integration

This guide explains how to integrate ferrite-sdk into a Zephyr RTOS project using the C FFI static library. You will cross-compile the Rust FFI crate, generate a C header, link the static library into your Zephyr build, implement the transport callback, and call the SDK from your Zephyr application.

## Overview

The `ferrite-ffi` crate produces a static library (`libferrite_ffi.a`) and exposes all SDK functionality through `extern "C"` functions. From the Zephyr side, you:

1. Download (or build) the pre-compiled `.a` for your target.
2. Copy the C header file into your project.
3. Add the library to your CMake build.
4. Implement the `send_chunk` callback using your Zephyr transport driver.
5. Call `ferrite_sdk_init()`, record metrics, and call `ferrite_upload()`.

## Step 1 -- Build the static library

From the ferrite-sdk repository root, build the FFI crate for your target:

```bash
# For Cortex-M4F (nRF52840, STM32F4, etc.)
cargo build -p ferrite-ffi \
  --target thumbv7em-none-eabihf \
  --release \
  --features cortex-m

# The output is at:
# target/thumbv7em-none-eabihf/release/libferrite_ffi.a
```

For Cortex-M3 targets (no hardware FPU):

```bash
cargo build -p ferrite-ffi \
  --target thumbv7m-none-eabi \
  --release \
  --features cortex-m
```

Copy the resulting `.a` file into your Zephyr project, e.g., `lib/libferrite_ffi.a`.

## Step 2 -- Create the C header

Create `include/ferrite_sdk.h` in your Zephyr project:

```c
#ifndef FERRITE_SDK_H
#define FERRITE_SDK_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ------------------------------------------------------------------ */
/* Error codes                                                         */
/* ------------------------------------------------------------------ */

typedef enum {
    FERRITE_OK              =  0,
    FERRITE_NOT_INITIALIZED = -1,
    FERRITE_ALREADY_INIT    = -2,
    FERRITE_BUFFER_FULL     = -3,
    FERRITE_KEY_TOO_LONG    = -4,
    FERRITE_NULL_PTR        = -5,
    FERRITE_ENCODING        = -6,
    FERRITE_TRANSPORT       = -7,
} ferrite_error_t;

/* ------------------------------------------------------------------ */
/* RAM region                                                          */
/* ------------------------------------------------------------------ */

typedef struct {
    uint32_t start;
    uint32_t end;
} ferrite_ram_region_t;

/* ------------------------------------------------------------------ */
/* Transport                                                           */
/* ------------------------------------------------------------------ */

/**
 * Callback to send a single binary chunk.
 *
 * @param data  Pointer to chunk bytes.
 * @param len   Number of bytes.
 * @param ctx   Opaque context pointer (passed through from ferrite_transport_t).
 * @return 0 on success, non-zero on error.
 */
typedef int32_t (*ferrite_send_chunk_fn)(const uint8_t *data, uint32_t len, void *ctx);

/**
 * Callback to query transport availability.
 *
 * @param ctx  Opaque context pointer.
 * @return true if the transport link is ready.
 */
typedef bool (*ferrite_is_available_fn)(void *ctx);

typedef struct {
    ferrite_send_chunk_fn  send_chunk;    /* Required */
    ferrite_is_available_fn is_available; /* Optional (NULL = always available) */
    void                 *ctx;          /* Opaque context forwarded to callbacks */
} ferrite_transport_t;

/* ------------------------------------------------------------------ */
/* Upload statistics                                                   */
/* ------------------------------------------------------------------ */

typedef struct {
    uint32_t chunks_sent;
    uint32_t bytes_sent;
    bool     fault_uploaded;
    uint32_t metrics_uploaded;
    uint32_t trace_bytes_uploaded;
} ferrite_upload_stats_t;

/* ------------------------------------------------------------------ */
/* Fault record                                                        */
/* ------------------------------------------------------------------ */

typedef struct {
    bool     valid;
    uint8_t  fault_type;     /* 0=HardFault, 1=MemFault, 2=BusFault, 3=UsageFault */
    uint8_t  _pad[2];
    /* Hardware exception frame */
    uint32_t r0, r1, r2, r3, r12, lr, pc, xpsr;
    /* Software-captured registers */
    uint32_t r4, r5, r6, r7, r8, r9, r10, r11, sp;
    /* Stack snapshot: first 16 words above SP at fault time */
    uint32_t stack_snapshot[16];
    /* Cortex-M fault status registers */
    uint32_t cfsr, hfsr, mmfar, bfar;
} ferrite_fault_record_t;

/* ------------------------------------------------------------------ */
/* SDK functions                                                       */
/* ------------------------------------------------------------------ */

/**
 * Initialize the SDK. Call once at firmware startup.
 *
 * @param device_id         NUL-terminated device identifier.
 * @param firmware_version  NUL-terminated firmware version string.
 * @param build_id          First 8 bytes of the ELF .build_id section.
 * @param ticks_fn          Function returning current monotonic tick count.
 * @param ram_regions       Pointer to array of valid RAM regions.
 * @param ram_region_count  Number of RAM regions.
 */
ferrite_error_t ferrite_sdk_init(
    const char           *device_id,
    const char           *firmware_version,
    uint64_t              build_id,
    uint64_t            (*ticks_fn)(void),
    const ferrite_ram_region_t *ram_regions,
    uint32_t              ram_region_count
);

/**
 * Record the reboot reason for the current boot cycle.
 *
 * Reason codes:
 *   0 = Unknown, 1 = PowerOnReset, 2 = SoftwareReset,
 *   3 = WatchdogTimeout, 4 = HardFault, 5 = MemoryFault,
 *   6 = BusFault, 7 = UsageFault, 8 = AssertFailed,
 *   9 = PinReset, 10 = BrownoutReset, 11 = FirmwareUpdate,
 *   12 = UserRequested.
 */
ferrite_error_t ferrite_record_reboot_reason(uint8_t reason);

/**
 * Retrieve the reboot reason from the previous boot.
 *
 * On success, writes the reason code to *out_reason.
 * If no valid record exists, writes 0 (Unknown).
 */
ferrite_error_t ferrite_last_reboot_reason(uint8_t *out_reason);

/**
 * Increment a counter metric by delta.
 *
 * @param key    NUL-terminated metric key (max 32 chars).
 * @param delta  Value to add.
 */
ferrite_error_t ferrite_metric_increment(const char *key, uint32_t delta);

/**
 * Set a gauge metric to value.
 *
 * @param key    NUL-terminated metric key (max 32 chars).
 * @param value  New gauge value.
 */
ferrite_error_t ferrite_metric_gauge(const char *key, float value);

/**
 * Record a histogram observation.
 *
 * @param key    NUL-terminated metric key (max 32 chars).
 * @param value  Observed value.
 */
ferrite_error_t ferrite_metric_observe(const char *key, float value);

/**
 * Retrieve the fault record from the previous boot.
 *
 * @param out  Pointer to fault record struct. out->valid is false if no fault.
 */
ferrite_error_t ferrite_last_fault(ferrite_fault_record_t *out);

/**
 * Run a full blocking upload session.
 *
 * @param transport  Transport descriptor with send_chunk callback.
 * @param out_stats  Optional pointer to receive upload statistics (may be NULL).
 */
ferrite_error_t ferrite_upload(
    const ferrite_transport_t *transport,
    ferrite_upload_stats_t    *out_stats
);

#ifdef __cplusplus
}
#endif

#endif /* FERRITE_SDK_H */
```

## Step 3 -- Add to your Zephyr CMake build

In your `CMakeLists.txt`:

```cmake
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})

project(my_app)

# Your application sources
target_sources(app PRIVATE src/main.c)

# Include path for ferrite_sdk.h
target_include_directories(app PRIVATE include)

# Link the pre-built static library
target_link_libraries(app PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/lib/libferrite_ffi.a
)
```

You also need to add the retained RAM section to your board's device tree overlay or linker script. For Zephyr, add to your `boards/my_board.overlay` or custom linker fragment:

```
/* In your Zephyr linker overlay (e.g., sections-rom.ld) */
SECTION_DATA_PROLOGUE(.uninit.ferrite, (NOLOAD),)
{
    . = ALIGN(4);
    KEEP(*(.uninit.ferrite))
    . = ALIGN(4);
} GROUP_DATA_LINK_IN(RETAINED, RETAINED)
```

Or use Zephyr's `__noinit` section and ensure it maps to a region that survives soft resets. The exact configuration depends on your SoC.

## Step 4 -- Implement the transport callback

Here is a complete UART transport implementation for Zephyr:

```c
#include <zephyr/kernel.h>
#include <zephyr/device.h>
#include <zephyr/drivers/uart.h>
#include "ferrite_sdk.h"

static const struct device *uart_dev;

/**
 * send_chunk callback: writes chunk bytes over UART.
 */
static int32_t uart_send_chunk(const uint8_t *data, uint32_t len, void *ctx)
{
    (void)ctx;

    for (uint32_t i = 0; i < len; i++) {
        uart_poll_out(uart_dev, data[i]);
    }

    return 0; /* success */
}

/**
 * is_available callback: UART is always available.
 */
static bool uart_is_available(void *ctx)
{
    (void)ctx;
    return uart_dev != NULL && device_is_ready(uart_dev);
}
```

For a BLE transport, you would implement `send_chunk` to write to a GATT characteristic and `is_available` to check the connection state.

## Step 5 -- Full Zephyr application

```c
#include <zephyr/kernel.h>
#include <zephyr/device.h>
#include <zephyr/drivers/uart.h>
#include "ferrite_sdk.h"

static const struct device *uart_dev;

/* Forward declarations for transport callbacks */
static int32_t uart_send_chunk(const uint8_t *data, uint32_t len, void *ctx);
static bool uart_is_available(void *ctx);

/* Ticks function: return Zephyr uptime in ticks */
static uint64_t get_ticks(void)
{
    return (uint64_t)k_uptime_ticks();
}

int main(void)
{
    /* Get UART device */
    uart_dev = DEVICE_DT_GET(DT_NODELABEL(uart0));
    if (!device_is_ready(uart_dev)) {
        printk("UART device not ready\n");
        return -1;
    }

    /* Initialize the SDK */
    ferrite_ram_region_t regions[] = {
        { .start = 0x20000000, .end = 0x20040000 },
    };

    ferrite_error_t err = ferrite_sdk_init(
        "zephyr-sensor-01",     /* device_id */
        "1.0.0",                /* firmware_version */
        0,                      /* build_id */
        get_ticks,              /* ticks_fn */
        regions,                /* ram_regions */
        1                       /* ram_region_count */
    );

    if (err != FERRITE_OK) {
        printk("ferrite init failed: %d\n", err);
        return -1;
    }

    /* Record reboot reason */
    ferrite_record_reboot_reason(1); /* PowerOnReset */

    /* Check if we have a fault from the previous boot */
    ferrite_fault_record_t fault;
    ferrite_last_fault(&fault);
    if (fault.valid) {
        printk("Previous fault: type=%u PC=0x%08X LR=0x%08X CFSR=0x%08X\n",
               fault.fault_type, fault.pc, fault.lr, fault.cfsr);
    }

    /* Set up transport */
    ferrite_transport_t transport = {
        .send_chunk   = uart_send_chunk,
        .is_available = uart_is_available,
        .ctx          = NULL,
    };

    /* Main loop */
    int iteration = 0;
    while (1) {
        /* Record some metrics */
        ferrite_metric_gauge("temperature", 23.5f + (float)(iteration % 10));
        ferrite_metric_increment("loop_count", 1);
        ferrite_metric_observe("process_time_us", 150.0f + (float)(iteration % 50));

        iteration++;

        /* Upload every 60 seconds */
        if (iteration % 60 == 0) {
            ferrite_upload_stats_t stats;
            ferrite_error_t upload_err = ferrite_upload(&transport, &stats);

            if (upload_err == FERRITE_OK) {
                printk("Upload OK: %u chunks, %u bytes\n",
                       stats.chunks_sent, stats.bytes_sent);
            } else {
                printk("Upload failed: %d\n", upload_err);
            }
        }

        k_sleep(K_SECONDS(1));
    }

    return 0;
}

/* Transport callback implementations */
static int32_t uart_send_chunk(const uint8_t *data, uint32_t len, void *ctx)
{
    (void)ctx;
    for (uint32_t i = 0; i < len; i++) {
        uart_poll_out(uart_dev, data[i]);
    }
    return 0;
}

static bool uart_is_available(void *ctx)
{
    (void)ctx;
    return uart_dev != NULL && device_is_ready(uart_dev);
}
```

## Retained RAM on Zephyr

Zephyr's default linker scripts zero-initialize all SRAM. To create a retained RAM region, you have several options:

### Option A: Custom linker fragment

Create `linker/retained.ld`:

```ld
SECTION_DATA_PROLOGUE(.uninit.ferrite, (NOLOAD),)
{
    . = ALIGN(4);
    _ferrite_retained_start = .;
    KEEP(*(.uninit.ferrite))
    _ferrite_retained_end = .;
    . = ALIGN(4);
} GROUP_DATA_LINK_IN(RAM, RAM)
```

Add to your `CMakeLists.txt`:

```cmake
zephyr_linker_sources(NOINIT linker/retained.ld)
```

### Option B: Device tree memory reservation

For some SoCs, you can reserve a memory region in the device tree:

```dts
/ {
    soc {
        ferrite_retained: memory@2003FF00 {
            compatible = "zephyr,memory-region";
            reg = <0x2003FF00 0x100>;
            zephyr,memory-region = "FERRITE_RETAINED";
        };
    };
};
```

## Error handling

All C FFI functions return `ferrite_error_t`. Check the return value after every call:

| Error | Value | Meaning |
|---|---|---|
| `FERRITE_OK` | 0 | Success |
| `FERRITE_NOT_INITIALIZED` | -1 | `ferrite_sdk_init()` was not called |
| `FERRITE_ALREADY_INIT` | -2 | `ferrite_sdk_init()` was called more than once |
| `FERRITE_BUFFER_FULL` | -3 | Metrics buffer is full (oldest entry was evicted) |
| `FERRITE_KEY_TOO_LONG` | -4 | Metric key exceeds 32 characters |
| `FERRITE_NULL_PTR` | -5 | A required pointer argument was NULL |
| `FERRITE_ENCODING` | -6 | Chunk encoding failed |
| `FERRITE_TRANSPORT` | -7 | Transport callback returned an error |

## Thread safety

The SDK uses critical sections internally. On Cortex-M, this means interrupts are briefly disabled during metric recording and upload orchestration. If you call SDK functions from multiple Zephyr threads, the critical sections ensure mutual exclusion. However, you should avoid calling `ferrite_upload()` from more than one thread simultaneously -- the upload session is not reentrant.
