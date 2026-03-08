use crate::store::Store;

/// Print a summary report of all devices and their event counts.
pub fn print_report(store: &Store) -> anyhow::Result<()> {
    let devices = store.list_devices()?;

    if devices.is_empty() {
        println!("No devices registered.");
        return Ok(());
    }

    println!(
        "{:<20} {:<15} {:<12} {:<8} {:<8} {:<8} {:<20}",
        "Device ID", "Firmware", "Build ID", "Faults", "Metrics", "Reboots", "Last Seen"
    );
    println!("{}", "-".repeat(91));

    for dev in &devices {
        let fault_count = store.count_faults_for_device(dev.id)?;
        let metric_count = store.count_metrics_for_device(dev.id)?;
        let reboot_count = store.count_reboots_for_device(dev.id)?;

        println!(
            "{:<20} {:<15} {:<12} {:<8} {:<8} {:<8} {:<20}",
            truncate(&dev.device_id, 19),
            truncate(&dev.firmware_version, 14),
            format!("0x{:X}", dev.build_id),
            fault_count,
            metric_count,
            reboot_count,
            &dev.last_seen,
        );
    }

    println!("\nTotal devices: {}", devices.len());
    Ok(())
}

/// Print recent fault events across all devices.
pub fn print_faults(store: &Store) -> anyhow::Result<()> {
    let faults = store.list_all_faults(50)?;

    if faults.is_empty() {
        println!("No fault events recorded.");
        return Ok(());
    }

    println!(
        "{:<20} {:<12} {:<12} {:<12} {:<30} {:<20}",
        "Device", "Type", "PC", "LR", "Symbol", "Time"
    );
    println!("{}", "-".repeat(106));

    for f in &faults {
        let fault_type_name = match f.fault_type {
            0 => "HardFault",
            1 => "MemFault",
            2 => "BusFault",
            3 => "UsageFault",
            _ => "Unknown",
        };

        println!(
            "{:<20} {:<12} 0x{:<10X} 0x{:<10X} {:<30} {:<20}",
            truncate(&f.device_id, 19),
            fault_type_name,
            f.pc,
            f.lr,
            truncate(f.symbol.as_deref().unwrap_or("-"), 29),
            &f.created_at,
        );
    }

    println!("\nShowing {} fault event(s).", faults.len());
    Ok(())
}

/// Print recent metrics across all devices.
pub fn print_metrics(store: &Store) -> anyhow::Result<()> {
    let metrics = store.list_all_metrics(50)?;

    if metrics.is_empty() {
        println!("No metrics recorded.");
        return Ok(());
    }

    println!(
        "{:<20} {:<25} {:<8} {:<35} {:<20}",
        "Device", "Key", "Type", "Value", "Time"
    );
    println!("{}", "-".repeat(108));

    for m in &metrics {
        let type_name = match m.metric_type {
            0 => "Counter",
            1 => "Gauge",
            2 => "Histo",
            _ => "?",
        };

        println!(
            "{:<20} {:<25} {:<8} {:<35} {:<20}",
            truncate(&m.device_id, 19),
            truncate(&m.key, 24),
            type_name,
            truncate(&m.value_json, 34),
            &m.created_at,
        );
    }

    println!("\nShowing {} metric row(s).", metrics.len());
    Ok(())
}

/// Truncate a string to at most `max_len` characters, appending "..." if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 3 {
        format!("{}...", &s[..max_len - 3])
    } else {
        s[..max_len].to_string()
    }
}
