#![no_std]
#![no_main]

use ferrite_sdk::{SdkConfig, RamRegion, RebootReason};
use defmt_rtt as _;
use panic_probe as _;
use stm32f4xx_hal as _;

mod build_id {
    pub fn get() -> u64 {
        env!("FERRITE_BUILD_ID").parse().unwrap_or(0)
    }
}

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true)]
mod app {
    use super::*;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        counter: u32,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        // Initialize SDK
        ferrite_sdk::init(SdkConfig {
            device_id: "stm32f4-example-01",
            firmware_version: env!("CARGO_PKG_VERSION"),
            build_id: build_id::get(),
            ticks_fn: || 0, // Use SysTick or TIM for real ticks
            ram_regions: &[RamRegion {
                start: 0x20000000,
                end: 0x20020000,
            }],
        });

        // Check for previous fault
        if let Some(fault) = ferrite_sdk::fault::last_fault() {
            defmt::error!(
                "Recovered from fault: PC={:#010x} LR={:#010x}",
                fault.frame.pc,
                fault.frame.lr
            );
        }

        ferrite_sdk::reboot_reason::record_reboot_reason(RebootReason::PowerOnReset);

        defmt::info!("RTIC STM32F4 example initialized");

        (Shared {}, Local { counter: 0 })
    }

    #[idle(local = [counter])]
    fn idle(cx: idle::Context) -> ! {
        loop {
            *cx.local.counter += 1;
            ferrite_sdk::metric_increment!("idle_count");

            // In a real application, trigger upload periodically here
            cortex_m::asm::wfi();
        }
    }
}
