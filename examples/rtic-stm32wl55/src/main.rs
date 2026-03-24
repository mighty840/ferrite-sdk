//! Ferrite SDK — NUCLEO-WL55JC1 with RTIC 2.x and USART transport.
//!
//! # Hardware
//! - Board: NUCLEO-WL55JC1
//! - MCU: STM32WL55JCIx (Cortex-M4 NO FPU, 32MHz HSE TCXO)
//! - Transport: USART2 PA2(TX)/PA3(RX) → ST-LINK VCP → RPi gateway
//! - Target: thumbv7em-none-eabi (soft-float, NO FPU)

#![no_std]
#![no_main]

use defmt_rtt as _;

// panic-probe causes reset loop without debugger — use halt instead
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop { cortex_m::asm::nop(); }
}

use stm32wl::stm32wl5x_cm4 as pac;

const DEVICE_ID: &str = "stm32wl55-rtic-01";

/// LPUART1 transport wrapper implementing ChunkTransport.
/// On NUCLEO-WL55JC1, the ST-LINK VCP is wired to LPUART1 (PA2/PA3 AF8).
pub struct Wl55Uart {
    lpuart: pac::LPUART,
}

impl Wl55Uart {
    fn new(lpuart: pac::LPUART) -> Self {
        Self { lpuart }
    }

    fn send_bytes(&mut self, data: &[u8]) {
        for &byte in data {
            while self.lpuart.isr.read().txfnf().bit_is_clear() {}
            self.lpuart.tdr.write(|w| w.tdr().bits(byte as u16));
        }
        while self.lpuart.isr.read().tc().bit_is_clear() {}
    }
}

#[derive(Debug)]
pub struct UartError;

impl ferrite_sdk::transport::ChunkTransport for Wl55Uart {
    type Error = UartError;

    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        self.send_bytes(chunk);
        Ok(())
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[rtic::app(device = stm32wl::stm32wl5x_cm4, peripherals = true, dispatchers = [SPI1, SPI2S2])]
mod app {
    use super::*;
    use ferrite_sdk::{RamRegion, RebootReason, SdkConfig};
    use ferrite_rtic::RticTransportResource;
    use rtic_monotonics::systick::prelude::*;

    rtic_monotonics::systick_monotonic!(Mono, 1000);

    #[shared]
    struct Shared {
        uploader: RticTransportResource<Wl55Uart>,
    }

    #[local]
    struct Local {
        counter: u32,
        led_blue: bool,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let dp = cx.device;

        // HSE 32MHz TCXO
        dp.RCC.cr.modify(|_, w| w.hsebyppwr().set_bit().hseon().set_bit());
        while dp.RCC.cr.read().hserdy().bit_is_clear() {}
        dp.RCC.cfgr.modify(|_, w| unsafe { w.sw().bits(0b10) });
        while dp.RCC.cfgr.read().sws().bits() != 0b10 {}

        // GPIO + LPUART1 clocks
        dp.RCC.ahb2enr.modify(|_, w| w.gpioaen().set_bit().gpioben().set_bit());
        dp.RCC.apb1enr2.modify(|_, w| w.lpuart1en().set_bit());

        // LPUART1: PA2=TX, PA3=RX, AF8 (VCP on NUCLEO-WL55JC1)
        dp.GPIOA.moder.modify(|_, w| unsafe { w.moder2().bits(0b10).moder3().bits(0b10) });
        dp.GPIOA.afrl.modify(|_, w| w.afrl2().af8().afrl3().af8());
        // LPUART BRR = 256 * fclk / baud = 256 * 32000000 / 115200 = 71111
        dp.LPUART.brr.write(|w| unsafe { w.bits(71111) });
        dp.LPUART.cr1.write(|w| w.te().set_bit().ue().set_bit());

        // LEDs: PB15 (blue), PB9 (green), PB11 (red)
        dp.GPIOB.moder.modify(|_, w| unsafe {
            w.moder15().bits(0b01).moder9().bits(0b01).moder11().bits(0b01)
        });

        // Ferrite SDK
        ferrite_sdk::init(SdkConfig {
            device_id: DEVICE_ID,
            firmware_version: env!("CARGO_PKG_VERSION"),
            build_id: 0,
            ticks_fn: || Mono::now().ticks() as u64,
            ram_regions: &[RamRegion {
                start: 0x20000000,
                end: 0x20010000,
            }],
        });

        let reason = read_wl55_reset_reason(&dp.RCC);
        ferrite_sdk::reboot_reason::record_reboot_reason(reason);

        if let Some(fault) = ferrite_sdk::fault::last_fault() {
            defmt::error!("Recovered from fault: PC={:#010x} LR={:#010x}", fault.frame.pc, fault.frame.lr);
        }

        Mono::start(cx.core.SYST, 32_000_000);

        let uart = Wl55Uart::new(dp.LPUART);
        let uploader = RticTransportResource::new(uart);

        defmt::info!("RTIC STM32WL55 started — device_id={}", DEVICE_ID);

        metric_tick::spawn().ok();

        (
            Shared { uploader },
            Local { counter: 0, led_blue: false },
        )
    }

    #[task(local = [counter, led_blue], shared = [uploader], priority = 2)]
    async fn metric_tick(mut cx: metric_tick::Context) {
        loop {
            Mono::delay(5000u32.millis()).await;

            *cx.local.counter += 1;
            let count = *cx.local.counter;

            // Blue LED heartbeat (PB15)
            *cx.local.led_blue = !*cx.local.led_blue;
            unsafe {
                let gpiob = &*pac::GPIOB::ptr();
                if *cx.local.led_blue {
                    gpiob.bsrr.write(|w| w.bs15().set_bit());
                } else {
                    gpiob.bsrr.write(|w| w.br15().set_bit());
                }
            }

            let _ = ferrite_sdk::metric_increment!("loop_count");
            let _ = ferrite_sdk::metric_gauge!("uptime_seconds", count * 5);

            // Integer-only temp simulation (no FPU on WL55 CM4)
            let temp = 25 + (count % 20) * 3 / 10;
            let _ = ferrite_sdk::metric_gauge!("mcu_temp", temp);

            defmt::trace!("metrics — iteration {}", count);

            // Upload every 30s
            if count % 6 == 0 {
                cx.shared.uploader.lock(|u| {
                    u.request_upload();
                });
                upload_poll::spawn().ok();
            }
        }
    }

    #[task(shared = [uploader], priority = 1)]
    async fn upload_poll(mut cx: upload_poll::Context) {
        // Green LED on during upload (PB9)
        unsafe { (*pac::GPIOB::ptr()).bsrr.write(|w| w.bs9().set_bit()); }

        let result = cx.shared.uploader.lock(|u| u.poll());

        unsafe { (*pac::GPIOB::ptr()).bsrr.write(|w| w.br9().set_bit()); }

        match result {
            Some(Ok(stats)) => {
                defmt::info!("upload OK: {} chunks, {} bytes", stats.chunks_sent, stats.bytes_sent);
                let _ = ferrite_sdk::metric_increment!("upload_ok");
            }
            Some(Err(_)) => {
                defmt::warn!("upload failed");
                let _ = ferrite_sdk::metric_increment!("upload_fail");
                // Red LED flash (PB11)
                unsafe { (*pac::GPIOB::ptr()).bsrr.write(|w| w.bs11().set_bit()); }
                Mono::delay(200u32.millis()).await;
                unsafe { (*pac::GPIOB::ptr()).bsrr.write(|w| w.br11().set_bit()); }
            }
            None => {}
        }
    }

    fn read_wl55_reset_reason(rcc: &pac::RCC) -> RebootReason {
        let csr = rcc.csr.read();
        rcc.csr.modify(|_, w| w.rmvf().set_bit());
        if csr.iwdgrstf().bit_is_set() {
            RebootReason::WatchdogTimeout
        } else if csr.wwdgrstf().bit_is_set() {
            RebootReason::WatchdogTimeout
        } else if csr.sftrstf().bit_is_set() {
            RebootReason::SoftwareReset
        } else if csr.pinrstf().bit_is_set() {
            RebootReason::PinReset
        } else {
            RebootReason::PowerOnReset
        }
    }
}
