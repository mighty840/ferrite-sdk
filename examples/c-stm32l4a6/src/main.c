/**
 * Ferrite SDK — C FFI example for Nucleo-L4A6ZG
 *
 * Demonstrates using ferrite-sdk from C via the FFI bindings.
 * Collects metrics and sends them over USART2 (PA2/PA3) to the
 * ferrite-gateway running on a connected Raspberry Pi.
 *
 * Hardware:
 *   Board:     Nucleo-L4A6ZG (Nucleo-144)
 *   MCU:       STM32L4A6ZGTx (Cortex-M4F, 80MHz)
 *   Transport: USART2 PA2(TX)/PA3(RX) → ST-LINK VCP → RPi gateway
 *   LEDs:      PB0 (green/LD1), PB7 (blue/LD2), PB14 (red/LD3)
 *
 * Transport path:
 *   [Nucleo USART2] → [ST-LINK VCP] → [RPi /dev/ttyACM0] → [gateway] → [server]
 */

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include "ferrite-sdk.h"

/* ── External symbols from system_stm32l4xx.c ─────────────────────── */

extern volatile uint32_t systick_ms;
extern uint64_t ferrite_ticks(void);

/* ── Critical-section implementation for Rust critical-section crate ── */
/* ferrite-sdk uses CriticalSectionMutex which needs these symbols.     */
/* On single-core Cortex-M, we disable interrupts via PRIMASK.          */

static uint32_t _cs_saved_primask;

bool _critical_section_1_0_acquire(void)
{
    uint32_t primask;
    __asm volatile ("mrs %0, PRIMASK" : "=r" (primask));
    __asm volatile ("cpsid i" ::: "memory");
    _cs_saved_primask = primask;
    return (primask & 1) == 0;  /* true if interrupts were enabled */
}

void _critical_section_1_0_release(bool _token)
{
    (void)_token;
    if ((_cs_saved_primask & 1) == 0) {
        __asm volatile ("cpsie i" ::: "memory");
    }
}

/* ── Register definitions ──────────────────────────────────────────── */

/* GPIO registers (STM32L4) */
#define GPIOA_BASE   0x48000000UL
#define GPIOB_BASE   0x48000400UL

#define GPIO_MODER(base)   (*(volatile uint32_t *)((base) + 0x00))
#define GPIO_OTYPER(base)  (*(volatile uint32_t *)((base) + 0x04))
#define GPIO_OSPEEDR(base) (*(volatile uint32_t *)((base) + 0x08))
#define GPIO_PUPDR(base)   (*(volatile uint32_t *)((base) + 0x0C))
#define GPIO_ODR(base)     (*(volatile uint32_t *)((base) + 0x14))
#define GPIO_BSRR(base)    (*(volatile uint32_t *)((base) + 0x18))
#define GPIO_AFRL(base)    (*(volatile uint32_t *)((base) + 0x20))

/* USART2 registers */
#define USART2_BASE  0x40004400UL
#define USART2_CR1   (*(volatile uint32_t *)(USART2_BASE + 0x00))
#define USART2_CR2   (*(volatile uint32_t *)(USART2_BASE + 0x04))
#define USART2_CR3   (*(volatile uint32_t *)(USART2_BASE + 0x08))
#define USART2_BRR   (*(volatile uint32_t *)(USART2_BASE + 0x0C))
#define USART2_ISR   (*(volatile uint32_t *)(USART2_BASE + 0x1C))
#define USART2_TDR   (*(volatile uint32_t *)(USART2_BASE + 0x28))

/* RCC_CSR for reboot reason */
#define RCC_CSR      (*(volatile uint32_t *)0x40021094UL)

/* ── LED helpers ───────────────────────────────────────────────────── */

static inline void led_green_on(void)  { GPIO_BSRR(GPIOB_BASE) = (1 << 0); }
static inline void led_green_off(void) { GPIO_BSRR(GPIOB_BASE) = (1 << 16); }
static inline void led_blue_on(void)   { GPIO_BSRR(GPIOB_BASE) = (1 << 7); }
static inline void led_blue_off(void)  { GPIO_BSRR(GPIOB_BASE) = (1 << 23); }
static inline void led_red_on(void)    { GPIO_BSRR(GPIOB_BASE) = (1 << 14); }
static inline void led_red_off(void)   { GPIO_BSRR(GPIOB_BASE) = (1 << 30); }

/* ── Delay ─────────────────────────────────────────────────────────── */

static void delay_ms(uint32_t ms) {
    uint32_t start = systick_ms;
    while ((systick_ms - start) < ms) {
        __asm volatile ("wfi");
    }
}

/* ── USART2 init and send ──────────────────────────────────────────── */

static void usart2_init(void) {
    /* PA2 = USART2_TX (AF7), PA3 = USART2_RX (AF7)
     * On Nucleo-144, PA2/PA3 are connected to ST-LINK VCP */

    /* PA2: alternate function mode (MODER = 10) */
    GPIO_MODER(GPIOA_BASE) &= ~(3 << (2 * 2));
    GPIO_MODER(GPIOA_BASE) |=  (2 << (2 * 2));
    /* PA3: alternate function mode */
    GPIO_MODER(GPIOA_BASE) &= ~(3 << (3 * 2));
    GPIO_MODER(GPIOA_BASE) |=  (2 << (3 * 2));

    /* AF7 for PA2 and PA3 */
    GPIO_AFRL(GPIOA_BASE) &= ~(0xF << (2 * 4));
    GPIO_AFRL(GPIOA_BASE) |=  (7   << (2 * 4));
    GPIO_AFRL(GPIOA_BASE) &= ~(0xF << (3 * 4));
    GPIO_AFRL(GPIOA_BASE) |=  (7   << (3 * 4));

    /* High speed for PA2 */
    GPIO_OSPEEDR(GPIOA_BASE) |= (3 << (2 * 2));

    /* USART2: 115200 baud at 80MHz APB1 */
    USART2_CR1 = 0;                  /* Disable while configuring */
    USART2_BRR = 694;                /* 80MHz / 115200 ≈ 694 */
    USART2_CR1 = (1 << 3)           /* TE: transmitter enable */
               | (1 << 0);          /* UE: USART enable */
}

static void usart2_send(const uint8_t *data, uint32_t len) {
    for (uint32_t i = 0; i < len; i++) {
        while (!(USART2_ISR & (1 << 7))) {}  /* Wait TXE */
        USART2_TDR = data[i];
    }
    while (!(USART2_ISR & (1 << 6))) {}      /* Wait TC */
}

/* ── Ferrite transport callbacks ───────────────────────────────────── */

static int32_t transport_send_chunk(const uint8_t *data, uint32_t len, void *ctx) {
    (void)ctx;
    usart2_send(data, len);
    return 0;  /* Success */
}

static bool transport_is_available(void *ctx) {
    (void)ctx;
    return true;  /* USART is always available */
}

/* ── LED setup ─────────────────────────────────────────────────────── */

static void leds_init(void) {
    /* PB0 (green), PB7 (blue), PB14 (red) — output push-pull */
    /* PB0 */
    GPIO_MODER(GPIOB_BASE) &= ~(3 << (0 * 2));
    GPIO_MODER(GPIOB_BASE) |=  (1 << (0 * 2));
    /* PB7 */
    GPIO_MODER(GPIOB_BASE) &= ~(3 << (7 * 2));
    GPIO_MODER(GPIOB_BASE) |=  (1 << (7 * 2));
    /* PB14 */
    GPIO_MODER(GPIOB_BASE) &= ~(3 << (14 * 2));
    GPIO_MODER(GPIOB_BASE) |=  (1 << (14 * 2));

    led_green_off();
    led_blue_off();
    led_red_off();
}

/* ── Reboot reason ─────────────────────────────────────────────────── */

static uint8_t read_reboot_reason(void) {
    uint32_t csr = RCC_CSR;
    /* Clear reset flags (RMVF bit 23) */
    RCC_CSR = csr | (1 << 23);

    if (csr & (1 << 29)) return 4;  /* IWDG → WatchdogTimeout */
    if (csr & (1 << 30)) return 4;  /* WWDG → WatchdogTimeout */
    if (csr & (1 << 28)) return 3;  /* SFTRSTF → SoftwareReset */
    if (csr & (1 << 26)) return 2;  /* PINRSTF → PinReset */
    return 1;                         /* PowerOnReset */
}

/* ── Main ──────────────────────────────────────────────────────────── */

int main(void) {
    leds_init();
    usart2_init();

    /* Green LED on during init */
    led_green_on();

    /* Initialize ferrite SDK via C FFI */
    ferrite_ram_region_t ram = { .start = 0x20000000, .end = 0x20050000 };

    ferrite_error_t err = ferrite_sdk_init(
        "stm32l4a6-c-01",       /* device_id */
        "0.1.0",                 /* firmware_version */
        __TIMESTAMP__[0],        /* build_id (crude but unique per build) */
        ferrite_ticks,           /* ticks function */
        &ram,                    /* RAM regions */
        1                        /* region count */
    );

    if (err != FERRITE_ERROR_T_OK) {
        /* Init failed — blink red LED forever */
        while (1) {
            led_red_on(); delay_ms(200);
            led_red_off(); delay_ms(200);
        }
    }

    /* Record reboot reason */
    ferrite_record_reboot_reason(read_reboot_reason());

    /* Check for previous fault */
    ferrite_fault_record_t fault;
    ferrite_last_fault(&fault);
    if (fault.valid) {
        /* Red LED flash to indicate recovered fault */
        for (int i = 0; i < 5; i++) {
            led_red_on(); delay_ms(100);
            led_red_off(); delay_ms(100);
        }
    }

    /* Set up transport descriptor */
    ferrite_transport_t transport = {
        .send_chunk   = transport_send_chunk,
        .is_available = transport_is_available,
        .ctx          = NULL,
    };

    led_green_off();

    /* ── Main loop ─────────────────────────────────────────────────── */

    uint32_t counter = 0;
    uint32_t upload_counter = 0;

    while (1) {
        counter++;

        /* Green LED heartbeat */
        led_green_on();
        delay_ms(50);
        led_green_off();

        /* Record metrics */
        ferrite_metric_increment("loop_count", 1);
        ferrite_metric_gauge("uptime_seconds", (float)(counter * 5));

        /* Simulated sensor values */
        ferrite_metric_gauge("cpu_temp", 42.0f + (float)(counter % 10) * 0.5f);
        ferrite_metric_gauge("vbat", 3.3f - (float)(counter % 100) * 0.001f);

        /* Upload every 30 seconds (6 iterations × 5s) */
        if (counter % 6 == 0) {
            upload_counter++;
            led_blue_on();

            ferrite_upload_stats_t stats;
            ferrite_error_t upload_err = ferrite_upload(&transport, &stats);

            if (upload_err == FERRITE_ERROR_T_OK) {
                ferrite_metric_increment("upload_ok", 1);
                ferrite_metric_gauge("upload_total", (float)upload_counter);
            } else {
                ferrite_metric_increment("upload_fail", 1);
                /* Red LED flash on upload error */
                led_red_on();
                delay_ms(200);
                led_red_off();
            }

            led_blue_off();
        }

        delay_ms(5000 - 50);  /* ~5s period accounting for heartbeat */
    }
}
