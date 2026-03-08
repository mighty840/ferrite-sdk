# FreeRTOS (C) Integration

This guide explains how to use the iotai-sdk C FFI library from a FreeRTOS project. The setup is similar to the [Zephyr integration](./zephyr-c) -- you link the same static library and use the same C header.

## Overview

FreeRTOS applications typically use a dedicated task for telemetry upload. The pattern is:

1. Call `iotai_sdk_init()` in `main()` before starting the scheduler.
2. Record metrics from any task or ISR.
3. Run `iotai_upload()` from a dedicated upload task on a periodic timer.

## Build and link

Build the static library for your target (see [Zephyr guide, Step 1](./zephyr-c#step-1----build-the-static-library)). Then add it to your build system.

### Makefile

```makefile
LIBS += lib/libiotai_sdk_ffi.a
CFLAGS += -Iinclude
```

### CMake

```cmake
add_executable(my_firmware src/main.c)
target_include_directories(my_firmware PRIVATE include)
target_link_libraries(my_firmware PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/lib/libiotai_sdk_ffi.a
)
```

Copy `iotai_sdk.h` from the [Zephyr guide](./zephyr-c#step-2----create-the-c-header) into your `include/` directory.

## FreeRTOS example

```c
#include "FreeRTOS.h"
#include "task.h"
#include "iotai_sdk.h"
#include "uart_driver.h"  /* Your UART HAL */

/* ---- Transport callback ---- */

static int32_t uart_send_chunk(const uint8_t *data, uint32_t len, void *ctx)
{
    (void)ctx;
    return uart_transmit(data, len) == 0 ? 0 : -1;
}

/* ---- Ticks function ---- */

static uint64_t get_ticks(void)
{
    return (uint64_t)xTaskGetTickCount();
}

/* ---- Upload task ---- */

static void upload_task(void *pvParameters)
{
    (void)pvParameters;

    iotai_transport_t transport = {
        .send_chunk   = uart_send_chunk,
        .is_available = NULL,  /* always available */
        .ctx          = NULL,
    };

    for (;;) {
        vTaskDelay(pdMS_TO_TICKS(60000)); /* 60 seconds */

        iotai_upload_stats_t stats;
        iotai_error_t err = iotai_upload(&transport, &stats);

        if (err == IOTAI_OK) {
            printf("Upload: %lu chunks, %lu bytes\n",
                   (unsigned long)stats.chunks_sent,
                   (unsigned long)stats.bytes_sent);
        }
    }
}

/* ---- Sensor task ---- */

static void sensor_task(void *pvParameters)
{
    (void)pvParameters;

    for (;;) {
        float temp = read_temperature_sensor();
        iotai_metric_gauge("temperature", temp);
        iotai_metric_increment("readings", 1);

        vTaskDelay(pdMS_TO_TICKS(1000));
    }
}

/* ---- Main ---- */

int main(void)
{
    hardware_init();

    /* Initialize the SDK before starting the scheduler */
    iotai_ram_region_t regions[] = {
        { .start = 0x20000000, .end = 0x20020000 },
    };

    iotai_sdk_init(
        "freertos-device",
        "2.0.0",
        0,
        get_ticks,
        regions,
        1
    );

    iotai_record_reboot_reason(1); /* PowerOnReset */

    /* Create tasks */
    xTaskCreate(sensor_task, "sensor", 256, NULL, 2, NULL);
    xTaskCreate(upload_task, "upload", 512, NULL, 1, NULL);

    /* Start scheduler */
    vTaskStartScheduler();

    /* Should not reach here */
    for (;;) {}
}
```

## Thread safety notes

- `iotai_metric_increment`, `iotai_metric_gauge`, and `iotai_metric_observe` are safe to call from multiple FreeRTOS tasks. The SDK uses Cortex-M critical sections (`CPSID I` / `CPSIE I`) internally.
- `iotai_upload()` must be called from only one task at a time. It is **not** reentrant.
- Do not call `iotai_upload()` from an ISR -- it performs blocking I/O through the transport callback.
- `iotai_metric_*` functions **can** be called from ISRs (they are interrupt-safe via critical sections), but keep ISR execution time in mind.

## Linker script for retained RAM

FreeRTOS projects typically use a custom linker script. Add the retained RAM section:

```ld
/* At the end of your MEMORY block */
RETAINED (rwx) : ORIGIN = 0x2001FF00, LENGTH = 0x100

/* In your SECTIONS block */
.uninit.iotai (NOLOAD) : {
    . = ALIGN(4);
    KEEP(*(.uninit.iotai))
    . = ALIGN(4);
} > RETAINED
```

Ensure your `_estack` and heap boundaries account for the 256 bytes reserved for retained RAM.

## Stack sizing

The `iotai_upload()` function uses approximately 400 bytes of stack space (for chunk encoding buffers). Size your upload task stack accordingly -- 512 words (2 KB) is a safe default.
