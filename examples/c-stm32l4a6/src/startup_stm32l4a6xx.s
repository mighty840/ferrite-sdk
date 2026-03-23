/**
 * Minimal startup for STM32L4A6xx — vector table + Reset_Handler
 *
 * Initializes .data (copy from FLASH), zeroes .bss, calls SystemInit + main.
 */

  .syntax unified
  .cpu cortex-m4
  .fpu softvfp
  .thumb

.global g_pfnVectors
.global Default_Handler

/* Linker symbols */
.word _sidata   /* Start of .data initializers in FLASH */
.word _sdata    /* Start of .data section in RAM */
.word _edata    /* End of .data section in RAM */
.word _sbss     /* Start of .bss section */
.word _ebss     /* End of .bss section */

  .section .text.Reset_Handler
  .weak Reset_Handler
  .type Reset_Handler, %function
Reset_Handler:
  ldr   sp, =_estack       /* Set stack pointer */

/* Copy .data from FLASH to RAM */
  movs  r1, #0
  b     LoopCopyDataInit
CopyDataInit:
  ldr   r3, =_sidata
  ldr   r3, [r3, r1]
  str   r3, [r0, r1]
  adds  r1, r1, #4
LoopCopyDataInit:
  ldr   r0, =_sdata
  ldr   r3, =_edata
  adds  r2, r0, r1
  cmp   r2, r3
  bcc   CopyDataInit

/* Zero .bss */
  ldr   r2, =_sbss
  ldr   r4, =_ebss
  movs  r3, #0
  b     LoopFillZerobss
FillZerobss:
  str   r3, [r2]
  adds  r2, r2, #4
LoopFillZerobss:
  cmp   r2, r4
  bcc   FillZerobss

/* Call SystemInit then main */
  bl    SystemInit
  bl    main
  bx    lr

.size Reset_Handler, .-Reset_Handler

/* Default handler for unhandled interrupts */
  .section .text.Default_Handler,"ax",%progbits
Default_Handler:
Infinite_Loop:
  b     Infinite_Loop
  .size Default_Handler, .-Default_Handler

/* ── Vector table ──────────────────────────────────────────────────── */

  .section .isr_vector,"a",%progbits
  .type g_pfnVectors, %object
  .size g_pfnVectors, .-g_pfnVectors

g_pfnVectors:
  .word _estack               /* 0x000  Initial SP */
  .word Reset_Handler         /* 0x004  Reset */
  .word NMI_Handler           /* 0x008  NMI */
  .word HardFault_Handler     /* 0x00C  Hard Fault */
  .word MemManage_Handler     /* 0x010  MPU Fault */
  .word BusFault_Handler      /* 0x014  Bus Fault */
  .word UsageFault_Handler    /* 0x018  Usage Fault */
  .word 0                     /* 0x01C  Reserved */
  .word 0                     /* 0x020  Reserved */
  .word 0                     /* 0x024  Reserved */
  .word 0                     /* 0x028  Reserved */
  .word SVC_Handler           /* 0x02C  SVCall */
  .word DebugMon_Handler      /* 0x030  Debug Monitor */
  .word 0                     /* 0x034  Reserved */
  .word PendSV_Handler        /* 0x038  PendSV */
  .word SysTick_Handler       /* 0x03C  SysTick */
  /* External interrupts — STM32L4A6 has 82 IRQs, we define common ones */
  .word WWDG_IRQHandler              /* 0  Window Watchdog */
  .word PVD_PVM_IRQHandler           /* 1  PVD/PVM */
  .word TAMP_STAMP_IRQHandler        /* 2  Tamper / Timestamp */
  .word RTC_WKUP_IRQHandler          /* 3  RTC Wakeup */
  .word FLASH_IRQHandler             /* 4  FLASH */
  .word RCC_IRQHandler               /* 5  RCC */
  .word EXTI0_IRQHandler             /* 6  EXTI Line 0 */
  .word EXTI1_IRQHandler             /* 7  EXTI Line 1 */
  .word EXTI2_IRQHandler             /* 8  EXTI Line 2 */
  .word EXTI3_IRQHandler             /* 9  EXTI Line 3 */
  .word EXTI4_IRQHandler             /* 10 EXTI Line 4 */
  .word DMA1_Channel1_IRQHandler     /* 11 DMA1 Ch1 */
  .word DMA1_Channel2_IRQHandler     /* 12 DMA1 Ch2 */
  .word DMA1_Channel3_IRQHandler     /* 13 DMA1 Ch3 */
  .word DMA1_Channel4_IRQHandler     /* 14 DMA1 Ch4 */
  .word DMA1_Channel5_IRQHandler     /* 15 DMA1 Ch5 */
  .word DMA1_Channel6_IRQHandler     /* 16 DMA1 Ch6 */
  .word DMA1_Channel7_IRQHandler     /* 17 DMA1 Ch7 */
  .word ADC1_2_IRQHandler            /* 18 ADC1/2 */
  .word CAN1_TX_IRQHandler           /* 19 CAN1 TX */
  .word CAN1_RX0_IRQHandler          /* 20 CAN1 RX0 */
  .word CAN1_RX1_IRQHandler          /* 21 CAN1 RX1 */
  .word CAN1_SCE_IRQHandler          /* 22 CAN1 SCE */
  .word EXTI9_5_IRQHandler           /* 23 EXTI 5-9 */
  .word TIM1_BRK_TIM15_IRQHandler    /* 24 TIM1 Break / TIM15 */
  .word TIM1_UP_TIM16_IRQHandler     /* 25 TIM1 Update / TIM16 */
  .word TIM1_TRG_COM_TIM17_IRQHandler /* 26 TIM1 Trigger / TIM17 */
  .word TIM1_CC_IRQHandler           /* 27 TIM1 Capture Compare */
  .word TIM2_IRQHandler              /* 28 TIM2 */
  .word TIM3_IRQHandler              /* 29 TIM3 */
  .word TIM4_IRQHandler              /* 30 TIM4 */
  .word I2C1_EV_IRQHandler           /* 31 I2C1 Event */
  .word I2C1_ER_IRQHandler           /* 32 I2C1 Error */
  .word I2C2_EV_IRQHandler           /* 33 I2C2 Event */
  .word I2C2_ER_IRQHandler           /* 34 I2C2 Error */
  .word SPI1_IRQHandler              /* 35 SPI1 */
  .word SPI2_IRQHandler              /* 36 SPI2 */
  .word USART1_IRQHandler            /* 37 USART1 */
  .word USART2_IRQHandler            /* 38 USART2 */
  .word USART3_IRQHandler            /* 39 USART3 */
  .word EXTI15_10_IRQHandler         /* 40 EXTI 10-15 */

/* ── Weak aliases — default to Default_Handler ──────────────────── */

  .weak NMI_Handler
  .thumb_set NMI_Handler, Default_Handler
  .weak HardFault_Handler
  .thumb_set HardFault_Handler, Default_Handler
  .weak MemManage_Handler
  .thumb_set MemManage_Handler, Default_Handler
  .weak BusFault_Handler
  .thumb_set BusFault_Handler, Default_Handler
  .weak UsageFault_Handler
  .thumb_set UsageFault_Handler, Default_Handler
  .weak SVC_Handler
  .thumb_set SVC_Handler, Default_Handler
  .weak DebugMon_Handler
  .thumb_set DebugMon_Handler, Default_Handler
  .weak PendSV_Handler
  .thumb_set PendSV_Handler, Default_Handler
  .weak SysTick_Handler
  .thumb_set SysTick_Handler, Default_Handler
  .weak WWDG_IRQHandler
  .thumb_set WWDG_IRQHandler, Default_Handler
  .weak PVD_PVM_IRQHandler
  .thumb_set PVD_PVM_IRQHandler, Default_Handler
  .weak TAMP_STAMP_IRQHandler
  .thumb_set TAMP_STAMP_IRQHandler, Default_Handler
  .weak RTC_WKUP_IRQHandler
  .thumb_set RTC_WKUP_IRQHandler, Default_Handler
  .weak FLASH_IRQHandler
  .thumb_set FLASH_IRQHandler, Default_Handler
  .weak RCC_IRQHandler
  .thumb_set RCC_IRQHandler, Default_Handler
  .weak EXTI0_IRQHandler
  .thumb_set EXTI0_IRQHandler, Default_Handler
  .weak EXTI1_IRQHandler
  .thumb_set EXTI1_IRQHandler, Default_Handler
  .weak EXTI2_IRQHandler
  .thumb_set EXTI2_IRQHandler, Default_Handler
  .weak EXTI3_IRQHandler
  .thumb_set EXTI3_IRQHandler, Default_Handler
  .weak EXTI4_IRQHandler
  .thumb_set EXTI4_IRQHandler, Default_Handler
  .weak DMA1_Channel1_IRQHandler
  .thumb_set DMA1_Channel1_IRQHandler, Default_Handler
  .weak DMA1_Channel2_IRQHandler
  .thumb_set DMA1_Channel2_IRQHandler, Default_Handler
  .weak DMA1_Channel3_IRQHandler
  .thumb_set DMA1_Channel3_IRQHandler, Default_Handler
  .weak DMA1_Channel4_IRQHandler
  .thumb_set DMA1_Channel4_IRQHandler, Default_Handler
  .weak DMA1_Channel5_IRQHandler
  .thumb_set DMA1_Channel5_IRQHandler, Default_Handler
  .weak DMA1_Channel6_IRQHandler
  .thumb_set DMA1_Channel6_IRQHandler, Default_Handler
  .weak DMA1_Channel7_IRQHandler
  .thumb_set DMA1_Channel7_IRQHandler, Default_Handler
  .weak ADC1_2_IRQHandler
  .thumb_set ADC1_2_IRQHandler, Default_Handler
  .weak CAN1_TX_IRQHandler
  .thumb_set CAN1_TX_IRQHandler, Default_Handler
  .weak CAN1_RX0_IRQHandler
  .thumb_set CAN1_RX0_IRQHandler, Default_Handler
  .weak CAN1_RX1_IRQHandler
  .thumb_set CAN1_RX1_IRQHandler, Default_Handler
  .weak CAN1_SCE_IRQHandler
  .thumb_set CAN1_SCE_IRQHandler, Default_Handler
  .weak EXTI9_5_IRQHandler
  .thumb_set EXTI9_5_IRQHandler, Default_Handler
  .weak TIM1_BRK_TIM15_IRQHandler
  .thumb_set TIM1_BRK_TIM15_IRQHandler, Default_Handler
  .weak TIM1_UP_TIM16_IRQHandler
  .thumb_set TIM1_UP_TIM16_IRQHandler, Default_Handler
  .weak TIM1_TRG_COM_TIM17_IRQHandler
  .thumb_set TIM1_TRG_COM_TIM17_IRQHandler, Default_Handler
  .weak TIM1_CC_IRQHandler
  .thumb_set TIM1_CC_IRQHandler, Default_Handler
  .weak TIM2_IRQHandler
  .thumb_set TIM2_IRQHandler, Default_Handler
  .weak TIM3_IRQHandler
  .thumb_set TIM3_IRQHandler, Default_Handler
  .weak TIM4_IRQHandler
  .thumb_set TIM4_IRQHandler, Default_Handler
  .weak I2C1_EV_IRQHandler
  .thumb_set I2C1_EV_IRQHandler, Default_Handler
  .weak I2C1_ER_IRQHandler
  .thumb_set I2C1_ER_IRQHandler, Default_Handler
  .weak I2C2_EV_IRQHandler
  .thumb_set I2C2_EV_IRQHandler, Default_Handler
  .weak I2C2_ER_IRQHandler
  .thumb_set I2C2_ER_IRQHandler, Default_Handler
  .weak SPI1_IRQHandler
  .thumb_set SPI1_IRQHandler, Default_Handler
  .weak SPI2_IRQHandler
  .thumb_set SPI2_IRQHandler, Default_Handler
  .weak USART1_IRQHandler
  .thumb_set USART1_IRQHandler, Default_Handler
  .weak USART2_IRQHandler
  .thumb_set USART2_IRQHandler, Default_Handler
  .weak USART3_IRQHandler
  .thumb_set USART3_IRQHandler, Default_Handler
  .weak EXTI15_10_IRQHandler
  .thumb_set EXTI15_10_IRQHandler, Default_Handler
