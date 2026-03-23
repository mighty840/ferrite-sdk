/**
 * Ferrite SDK — Zephyr BLE example for nRF5340-DK
 *
 * Demonstrates using ferrite-sdk from C (via FFI) on Zephyr RTOS,
 * transmitting telemetry chunks over BLE GATT notifications.
 *
 * Hardware:
 *   Board:     nRF5340-DK (PCA10095)
 *   MCU:       nRF5340 application core (Cortex-M33, 128MHz)
 *   Transport: BLE GATT notifications → RPi gateway (btleplug)
 *   LEDs:      P0.28 (LED1), P0.29 (LED2), P0.30 (LED3), P0.31 (LED4)
 *
 * Transport path:
 *   [nRF5340 BLE] ──GATT notify──▶ [RPi gateway BLE scanner] → [server]
 *
 * BLE UUIDs (match ferrite-ble-nrf and gateway ble_scanner.rs):
 *   Service:        FE771E00-0001-4000-8000-00805F9B34FB
 *   Chunk char:     FE771E00-0002-4000-8000-00805F9B34FB
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/bluetooth/bluetooth.h>
#include <zephyr/bluetooth/gatt.h>
#include <zephyr/bluetooth/uuid.h>
#include <zephyr/bluetooth/conn.h>

#include "ferrite-sdk.h"

LOG_MODULE_REGISTER(ferrite, LOG_LEVEL_INF);

/* ── Configuration ─────────────────────────────────────────────────── */

#define DEVICE_ID       "nrf5340-zephyr-01"
#define FW_VERSION      "0.1.0"
#define METRIC_INTERVAL_S  5
#define UPLOAD_INTERVAL_S  30

/* ── BLE UUIDs — must match ferrite-ble-nrf and gateway ────────────── */

/* Ferrite service: FE771E00-0001-4000-8000-00805F9B34FB */
static struct bt_uuid_128 ferrite_svc_uuid = BT_UUID_INIT_128(
    BT_UUID_128_ENCODE(0xFE771E00, 0x0001, 0x4000, 0x8000, 0x00805F9B34FB));

/* Chunk characteristic: FE771E00-0002-4000-8000-00805F9B34FB */
static struct bt_uuid_128 chunk_char_uuid = BT_UUID_INIT_128(
    BT_UUID_128_ENCODE(0xFE771E00, 0x0002, 0x4000, 0x8000, 0x00805F9B34FB));

/* ── BLE state ─────────────────────────────────────────────────────── */

static struct bt_conn *current_conn;
static bool notifications_enabled;

/* CCC (Client Characteristic Configuration) changed callback */
static void chunk_ccc_changed(const struct bt_gatt_attr *attr, uint16_t value)
{
    notifications_enabled = (value == BT_GATT_CCC_NOTIFY);
    LOG_INF("BLE: notifications %s", notifications_enabled ? "enabled" : "disabled");
}

/* GATT service definition */
BT_GATT_SERVICE_DEFINE(ferrite_svc,
    BT_GATT_PRIMARY_SERVICE(&ferrite_svc_uuid),
    BT_GATT_CHARACTERISTIC(&chunk_char_uuid.uuid,
        BT_GATT_CHRC_NOTIFY,
        BT_GATT_PERM_NONE,
        NULL, NULL, NULL),
    BT_GATT_CCC(chunk_ccc_changed, BT_GATT_PERM_READ | BT_GATT_PERM_WRITE),
);

/* ── BLE advertising data ──────────────────────────────────────────── */

static const struct bt_data ad[] = {
    BT_DATA_BYTES(BT_DATA_FLAGS, (BT_LE_AD_GENERAL | BT_LE_AD_NO_BREDR)),
    BT_DATA_BYTES(BT_DATA_UUID128_ALL,
        BT_UUID_128_ENCODE(0xFE771E00, 0x0001, 0x4000, 0x8000, 0x00805F9B34FB)),
};

static const struct bt_data sd[] = {
    BT_DATA(BT_DATA_NAME_COMPLETE, DEVICE_ID, sizeof(DEVICE_ID) - 1),
};

/* ── BLE connection callbacks ──────────────────────────────────────── */

static void connected(struct bt_conn *conn, uint8_t err)
{
    if (err) {
        LOG_ERR("BLE: connection failed (err %u)", err);
        return;
    }
    LOG_INF("BLE: connected");
    current_conn = bt_conn_ref(conn);
}

static void disconnected(struct bt_conn *conn, uint8_t reason)
{
    LOG_INF("BLE: disconnected (reason %u)", reason);
    if (current_conn) {
        bt_conn_unref(current_conn);
        current_conn = NULL;
    }
    notifications_enabled = false;
}

BT_CONN_CB_DEFINE(conn_callbacks) = {
    .connected = connected,
    .disconnected = disconnected,
};

/* ── LEDs ──────────────────────────────────────────────────────────── */

/* nRF5340-DK LEDs are active LOW on P0.28-P0.31 */
static const struct gpio_dt_spec led1 = GPIO_DT_SPEC_GET(DT_ALIAS(led0), gpios);
static const struct gpio_dt_spec led2 = GPIO_DT_SPEC_GET(DT_ALIAS(led1), gpios);
static const struct gpio_dt_spec led3 = GPIO_DT_SPEC_GET(DT_ALIAS(led2), gpios);
static const struct gpio_dt_spec led4 = GPIO_DT_SPEC_GET(DT_ALIAS(led3), gpios);

static void leds_init(void)
{
    gpio_pin_configure_dt(&led1, GPIO_OUTPUT_INACTIVE);
    gpio_pin_configure_dt(&led2, GPIO_OUTPUT_INACTIVE);
    gpio_pin_configure_dt(&led3, GPIO_OUTPUT_INACTIVE);
    gpio_pin_configure_dt(&led4, GPIO_OUTPUT_INACTIVE);
}

/* ── Ferrite transport: BLE GATT notifications ─────────────────────── */

static int32_t ble_send_chunk(const uint8_t *data, uint32_t len, void *ctx)
{
    (void)ctx;

    if (!current_conn || !notifications_enabled) {
        return -1;  /* No connection or notifications not enabled */
    }

    /* Get the attribute handle for the chunk characteristic value.
     * BT_GATT_SERVICE_DEFINE creates attrs in order:
     *   [0] = service declaration
     *   [1] = characteristic declaration
     *   [2] = characteristic value
     *   [3] = CCC descriptor
     */
    const struct bt_gatt_attr *chunk_attr = &ferrite_svc.attrs[2];

    int err = bt_gatt_notify(current_conn, chunk_attr, data, len);
    if (err) {
        LOG_WRN("BLE: notify failed (err %d)", err);
        return err;
    }

    return 0;
}

static bool ble_is_available(void *ctx)
{
    (void)ctx;
    return current_conn != NULL && notifications_enabled;
}

/* ── Ticks function for ferrite SDK ────────────────────────────────── */

static uint64_t zephyr_ticks(void)
{
    return (uint64_t)k_uptime_get();
}

/* ── Main ──────────────────────────────────────────────────────────── */

int main(void)
{
    int err;

    LOG_INF("Ferrite Zephyr BLE example starting — %s", DEVICE_ID);

    leds_init();

    /* LED1 on during init */
    gpio_pin_set_dt(&led1, 1);

    /* Initialize ferrite SDK */
    ferrite_ram_region_t ram = {
        .start = 0x20000000,
        .end   = 0x20080000,  /* 512KB app core RAM */
    };

    ferrite_error_t ferr = ferrite_sdk_init(
        DEVICE_ID,
        FW_VERSION,
        0,            /* build_id — set by build system in production */
        zephyr_ticks,
        &ram,
        1
    );

    if (ferr != FERRITE_ERROR_OK) {
        LOG_ERR("ferrite_sdk_init failed: %d", ferr);
        while (1) {
            gpio_pin_toggle_dt(&led4);  /* Red LED blink on error */
            k_msleep(200);
        }
    }

    /* Record reboot reason */
    ferrite_record_reboot_reason(1);  /* PowerOnReset */

    /* Check for previous fault */
    ferrite_fault_record_t fault;
    ferrite_last_fault(&fault);
    if (fault.valid) {
        LOG_ERR("Recovered from fault: PC=0x%08x LR=0x%08x", fault.pc, fault.lr);
        for (int i = 0; i < 5; i++) {
            gpio_pin_set_dt(&led4, 1);
            k_msleep(100);
            gpio_pin_set_dt(&led4, 0);
            k_msleep(100);
        }
    }

    /* Initialize BLE */
    err = bt_enable(NULL);
    if (err) {
        LOG_ERR("BLE init failed (err %d)", err);
        return err;
    }
    LOG_INF("BLE initialized");

    /* Start advertising */
    err = bt_le_adv_start(BT_LE_ADV_CONN, ad, ARRAY_SIZE(ad), sd, ARRAY_SIZE(sd));
    if (err) {
        LOG_ERR("BLE advertising failed (err %d)", err);
        return err;
    }
    LOG_INF("BLE advertising started — waiting for gateway connection");

    gpio_pin_set_dt(&led1, 0);  /* Init complete */

    /* Set up transport */
    ferrite_transport_t transport = {
        .send_chunk   = ble_send_chunk,
        .is_available = ble_is_available,
        .ctx          = NULL,
    };

    /* ── Main telemetry loop ───────────────────────────────────────── */

    uint32_t counter = 0;
    uint32_t upload_counter = 0;

    while (1) {
        counter++;

        /* LED1 heartbeat: double blink pattern */
        gpio_pin_set_dt(&led1, 1);
        k_msleep(50);
        gpio_pin_set_dt(&led1, 0);
        k_msleep(100);
        gpio_pin_set_dt(&led1, 1);
        k_msleep(50);
        gpio_pin_set_dt(&led1, 0);

        /* Record metrics */
        ferrite_metric_increment("loop_count", 1);
        ferrite_metric_gauge("uptime_seconds",
                             (float)(counter * METRIC_INTERVAL_S));

        /* BLE connection status metric */
        ferrite_metric_gauge("ble_connected",
                             (current_conn != NULL) ? 1.0f : 0.0f);

        LOG_DBG("metrics recorded — iteration %u (BLE: %s)",
                counter,
                current_conn ? "connected" : "disconnected");

        /* Upload every UPLOAD_INTERVAL_S */
        if (counter % (UPLOAD_INTERVAL_S / METRIC_INTERVAL_S) == 0) {
            if (ble_is_available(NULL)) {
                upload_counter++;
                gpio_pin_set_dt(&led2, 1);  /* LED2 on during upload */

                ferrite_upload_stats_t stats;
                ferrite_error_t uerr = ferrite_upload(&transport, &stats);

                gpio_pin_set_dt(&led2, 0);

                if (uerr == FERRITE_ERROR_OK) {
                    LOG_INF("upload #%u OK: %u chunks, %u bytes",
                            upload_counter, stats.chunks_sent, stats.bytes_sent);
                    ferrite_metric_increment("upload_ok", 1);
                } else {
                    LOG_WRN("upload #%u failed: %d", upload_counter, uerr);
                    ferrite_metric_increment("upload_fail", 1);
                    gpio_pin_set_dt(&led4, 1);
                    k_msleep(200);
                    gpio_pin_set_dt(&led4, 0);
                }
            } else {
                LOG_DBG("upload skipped — BLE not available");
                /* LED3 blink = waiting for connection */
                gpio_pin_set_dt(&led3, 1);
                k_msleep(100);
                gpio_pin_set_dt(&led3, 0);
            }
        }

        k_msleep(METRIC_INTERVAL_S * 1000 - 200);  /* Account for LED timing */
    }

    return 0;
}
