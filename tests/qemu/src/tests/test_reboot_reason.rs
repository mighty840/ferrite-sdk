use iotai_sdk::reboot_reason::{RebootReason, record_reboot_reason, last_reboot_reason, clear_reboot_reason};

pub fn roundtrip() -> Result<(), &'static str> {
    record_reboot_reason(RebootReason::WatchdogTimeout);
    match last_reboot_reason() {
        Some(RebootReason::WatchdogTimeout) => {}
        Some(_) => return Err("wrong reason returned"),
        None => return Err("no reason returned"),
    }

    clear_reboot_reason();
    if last_reboot_reason().is_some() {
        return Err("reason should be None after clear");
    }

    Ok(())
}
