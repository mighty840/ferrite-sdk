#![no_std]
#![no_main]

use cortex_m_rt::entry;
use cortex_m_semihosting::{hprintln, debug};
use panic_semihosting as _;

mod tests;
mod transport;

#[entry]
fn main() -> ! {
    hprintln!("=== ferrite-sdk QEMU integration tests ===");

    let mut passed = 0u32;
    let mut failed = 0u32;

    macro_rules! run_test {
        ($name:expr, $fn:expr) => {
            hprintln!("  running {} ...", $name);
            match $fn() {
                Ok(()) => {
                    hprintln!("  ok");
                    passed += 1;
                }
                Err(msg) => {
                    hprintln!("  FAILED: {}", msg);
                    failed += 1;
                }
            }
        };
    }

    run_test!("metrics::counter_increment", tests::test_metrics::counter_increment);
    run_test!("metrics::gauge_overwrite", tests::test_metrics::gauge_overwrite);
    run_test!("metrics::histogram_accumulate", tests::test_metrics::histogram_accumulate);
    run_test!("metrics::buffer_full_evict", tests::test_metrics::buffer_full_evict);
    run_test!("trace::write_and_iterate", tests::test_trace::write_and_iterate);
    run_test!("trace::overflow_wrap", tests::test_trace::overflow_wrap);
    run_test!("chunks::encode_decode_metrics", tests::test_chunks::encode_decode_metrics);
    run_test!("chunks::encode_decode_fault", tests::test_chunks::encode_decode_fault);
    run_test!("chunks::crc_mismatch_detected", tests::test_chunks::crc_mismatch_detected);
    run_test!("reboot_reason::roundtrip", tests::test_reboot_reason::roundtrip);

    hprintln!("=== {} passed, {} failed ===", passed, failed);

    if failed == 0 {
        debug::exit(debug::EXIT_SUCCESS);
    } else {
        debug::exit(debug::EXIT_FAILURE);
    }

    loop {}
}
