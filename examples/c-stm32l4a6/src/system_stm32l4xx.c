/**
 * system_stm32l4xx.c — Minimal system init for STM32L4A6ZG
 *
 * Configures:
 *  - Flash latency for 80MHz
 *  - HSE (8MHz from ST-LINK MCO) → PLL → 80MHz SYSCLK
 *  - APB1/APB2 clocks
 *  - SysTick at 1ms
 */

#include <stdint.h>

/* ── Register addresses ─────────────────────────────────────────────── */

#define RCC_BASE        0x40021000UL
#define FLASH_BASE_ADDR 0x40022000UL

#define RCC_CR          (*(volatile uint32_t *)(RCC_BASE + 0x00))
#define RCC_CFGR        (*(volatile uint32_t *)(RCC_BASE + 0x08))
#define RCC_PLLCFGR     (*(volatile uint32_t *)(RCC_BASE + 0x0C))
#define RCC_AHB2ENR     (*(volatile uint32_t *)(RCC_BASE + 0x4C))
#define RCC_APB1ENR1    (*(volatile uint32_t *)(RCC_BASE + 0x58))
#define FLASH_ACR       (*(volatile uint32_t *)(FLASH_BASE_ADDR + 0x00))

/* SysTick */
#define SYST_CSR        (*(volatile uint32_t *)0xE000E010)
#define SYST_RVR        (*(volatile uint32_t *)0xE000E014)
#define SYST_CVR        (*(volatile uint32_t *)0xE000E018)

/* ── Globals ────────────────────────────────────────────────────────── */

uint32_t SystemCoreClock = 80000000UL;
volatile uint32_t systick_ms = 0;

/* ── SysTick handler — increments millisecond counter ───────────────── */

void SysTick_Handler(void) {
    systick_ms++;
}

/* Monotonic tick function for ferrite SDK */
uint64_t ferrite_ticks(void) {
    return (uint64_t)systick_ms;
}

/* ── SystemInit — called from startup before main ────────────────────── */

void SystemInit(void) {
    /* Enable HSE (8MHz bypass from ST-LINK MCO) */
    RCC_CR |= (1 << 16);        /* HSEON */
    RCC_CR |= (1 << 18);        /* HSEBYP — external clock, not crystal */
    while (!(RCC_CR & (1 << 17))) {} /* Wait for HSERDY */

    /* Configure PLL: HSE/1 * 20 /2 = 80MHz */
    RCC_CR &= ~(1 << 24);       /* PLLON off */
    while (RCC_CR & (1 << 25)) {} /* Wait PLL unlocked */

    /* PLLCFGR: PLLSRC=HSE(3), PLLM=0(/1), PLLN=20, PLLR=0(/2), PLLREN=1 */
    RCC_PLLCFGR = (3 << 0)        /* PLLSRC = HSE */
                | (0 << 4)        /* PLLM = /1 */
                | (20 << 8)       /* PLLN = 20 → 8*20=160MHz VCO */
                | (0 << 25)       /* PLLR = /2 → 80MHz SYSCLK */
                | (1 << 24);      /* PLLREN = enable R output */

    RCC_CR |= (1 << 24);        /* PLLON */
    while (!(RCC_CR & (1 << 25))) {} /* Wait PLLRDY */

    /* Flash latency: 4 wait states for 80MHz at 3.3V */
    FLASH_ACR = (FLASH_ACR & ~0xF) | 4;
    /* Enable prefetch and instruction cache */
    FLASH_ACR |= (1 << 8) | (1 << 9);

    /* Switch SYSCLK to PLL */
    RCC_CFGR = (RCC_CFGR & ~0x3) | 0x3;  /* SW = PLL */
    while ((RCC_CFGR & 0xC) != 0xC) {}    /* Wait SWS = PLL */

    /* APB1 = 80MHz (no divider), APB2 = 80MHz (no divider) */

    /* Enable GPIO port clocks: GPIOA (USART2), GPIOB (LEDs) */
    RCC_AHB2ENR |= (1 << 0) | (1 << 1);  /* GPIOAEN | GPIOBEN */

    /* Enable USART2 clock (APB1) */
    RCC_APB1ENR1 |= (1 << 17);  /* USART2EN */

    /* Configure SysTick for 1ms interrupts at 80MHz */
    SYST_RVR = 80000 - 1;       /* Reload value: 80MHz / 1000 */
    SYST_CVR = 0;               /* Clear current value */
    SYST_CSR = 7;               /* Enable, interrupt, processor clock */
}
