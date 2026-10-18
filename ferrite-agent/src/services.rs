//! Service health: watched systemd units, failed units, core dumps.
//!
//! Everything here works unprivileged: `systemctl show`/`list-units` read
//! state over D-Bus, and systemd-coredump's directory is world-listable.

use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use crate::config::Config;
use crate::system::{Sample, Value};

/// `(active, restarts)` from `systemctl show -p LoadState -p ActiveState -p
/// NRestarts`, or `None` if systemd doesn't know the unit.
pub fn parse_unit_show(text: &str) -> Option<(bool, u32)> {
    let mut active = false;
    let mut restarts = 0;
    for line in text.lines() {
        match line.split_once('=') {
            Some(("LoadState", "not-found")) => return None,
            Some(("ActiveState", v)) => active = v.trim() == "active",
            Some(("NRestarts", v)) => restarts = v.trim().parse().unwrap_or(0),
            _ => {}
        }
    }
    Some((active, restarts))
}

/// Core dumps written during the current boot. systemd-coredump names files
/// `core.<comm>.<uid>.<boot_id>.<pid>.<usec>[.zst|.xz|.lz4]`, with the boot ID
/// as 32 hex digits.
pub fn count_coredumps(dir: &Path, boot_id: &str) -> Option<u32> {
    let boot = boot_id.trim().replace('-', "");
    if boot.is_empty() {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let n = entries
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("core.") && name.split('.').any(|part| part == boot)
        })
        .count();
    Some(n as u32)
}

fn warn_once(unit: &str) {
    static WARNED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let mut warned = WARNED.lock().unwrap_or_else(|e| e.into_inner());
    if !warned.iter().any(|u| u == unit) {
        tracing::warn!(unit, "unit not found; not reported");
        warned.push(unit.to_string());
    }
}

fn systemctl(args: &[&str]) -> Option<String> {
    let out = Command::new("systemctl").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn sample(cfg: &Config) -> Vec<Sample> {
    let mut out = Vec::new();
    for unit in &cfg.units {
        let key = unit.metric_key();
        if let Some(text) = systemctl(&[
            "show",
            &unit.name,
            "--property=LoadState",
            "--property=ActiveState",
            "--property=NRestarts",
        ]) {
            let Some((active, restarts)) = parse_unit_show(&text) else {
                warn_once(&unit.name);
                continue;
            };
            out.push((
                format!("{key}_active"),
                Value::Gauge(if active { 1.0 } else { 0.0 }),
            ));
            out.push((format!("{key}_restarts"), Value::Counter(restarts)));
        }
    }
    if let Some(text) = systemctl(&["list-units", "--state=failed", "--no-legend", "--plain"]) {
        let failed = text.lines().filter(|l| !l.trim().is_empty()).count();
        out.push(("units_failed".into(), Value::Gauge(failed as f32)));
    }
    let boot_id = std::fs::read_to_string(cfg.host_path("/proc/sys/kernel/random/boot_id"))
        .unwrap_or_default();
    if let Some(n) = count_coredumps(&cfg.host_path("/var/lib/systemd/coredump"), &boot_id) {
        out.push(("coredumps".into(), Value::Counter(n)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_show_output() {
        let show = |t: &str| parse_unit_show(t);
        assert_eq!(
            show("LoadState=loaded\nNRestarts=3\nActiveState=active\n"),
            Some((true, 3))
        );
        assert_eq!(show("ActiveState=failed\nNRestarts=0\n"), Some((false, 0)));
        assert_eq!(show("ActiveState=activating\n"), Some((false, 0)));
        assert_eq!(
            show("LoadState=not-found\nActiveState=inactive\nNRestarts=0\n"),
            None
        );
    }

    #[test]
    fn coredumps_of_this_boot_only() {
        let dir = tempfile::tempdir().unwrap();
        let this = "6f2c0d6e-1b7a-4b0e-9c4f-3a5d2e1f0a9b";
        let this_hex = this.replace('-', "");
        for name in [
            format!("core.codesys.0.{this_hex}.812.1760780000000000.zst"),
            format!("core.my\\x2dapp.1000.{this_hex}.900.1760780001000000.xz"),
            "core.codesys.0.00000000000000000000000000000000.1.1.zst".to_string(),
            format!("notes.{this_hex}"),
        ] {
            std::fs::write(dir.path().join(name), b"").unwrap();
        }
        assert_eq!(count_coredumps(dir.path(), this), Some(2));
        assert_eq!(count_coredumps(dir.path(), ""), None);
        assert_eq!(count_coredumps(&dir.path().join("nope"), this), None);
    }
}
