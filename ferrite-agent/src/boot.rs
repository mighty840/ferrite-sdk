//! Reboot detection and classification.
//!
//! The agent remembers the kernel's `boot_id` in its state directory. When the
//! ID changes, the device rebooted, and the reason maps onto the SDK's
//! [`RebootReason`] codes so Linux devices show up like MCUs on the server:
//!
//! | Evidence | Reason |
//! |---|---|
//! | kernel panic/oops record in pstore since the last run | `HardFault` |
//! | watchdog `bootstatus` has `WDIOF_CARDRESET` | `WatchdogTimeout` |
//! | image `build_id` changed | `FirmwareUpdate` |
//! | the agent saw an orderly system shutdown | `SoftwareReset` |
//! | none of the above (power cut, hang, reset button) | `Unknown` |

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use ferrite_sdk::reboot_reason::RebootReason;
use serde::{Deserialize, Serialize};

use crate::config::Config;

/// Watchdog `bootstatus` bit: the last reboot was caused by the watchdog.
const WDIOF_CARDRESET: u32 = 0x0020;

/// Persisted between agent runs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BootState {
    pub boot_id: String,
    pub boot_sequence: u32,
    pub build_id: u64,
    /// Uptime the agent last recorded during `boot_id`.
    pub last_uptime_secs: u32,
    /// Unix time of the last save.
    pub saved_at: u64,
    /// The agent stopped because the system was shutting down.
    pub clean_shutdown: bool,
}

/// A reboot to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reboot {
    pub reason: RebootReason,
    pub boot_sequence: u32,
    pub uptime_before_reboot: u32,
}

/// What was found on this boot that bears on why the last one ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Evidence {
    pub panic_record: bool,
    pub watchdog_reset: bool,
    pub build_changed: bool,
    pub clean_shutdown: bool,
}

pub fn classify(e: Evidence) -> RebootReason {
    if e.panic_record {
        RebootReason::HardFault
    } else if e.watchdog_reset {
        RebootReason::WatchdogTimeout
    } else if e.build_changed {
        RebootReason::FirmwareUpdate
    } else if e.clean_shutdown {
        RebootReason::SoftwareReset
    } else {
        RebootReason::Unknown
    }
}

pub struct BootTracker {
    path: PathBuf,
    pub state: BootState,
}

impl BootTracker {
    /// Load state and compare with the running boot. Returns the reboot to
    /// report, if the device rebooted since the agent last ran.
    pub fn start(cfg: &Config, build_id: u64, uptime_secs: u32) -> Result<(Self, Option<Reboot>)> {
        let path = cfg.state_dir.join("boot.json");
        let boot_id = std::fs::read_to_string(cfg.host_path("/proc/sys/kernel/random/boot_id"))
            .context("reading boot_id")?
            .trim()
            .to_string();
        let previous: Option<BootState> = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());

        let (state, reboot) = match previous {
            // Agent restarted within the same boot.
            Some(prev) if prev.boot_id == boot_id => (
                BootState {
                    build_id,
                    clean_shutdown: false,
                    ..prev
                },
                None,
            ),
            Some(prev) => {
                let evidence = Evidence {
                    panic_record: panic_record_since(cfg, prev.saved_at),
                    watchdog_reset: watchdog_reset(cfg),
                    build_changed: prev.build_id != build_id,
                    clean_shutdown: prev.clean_shutdown,
                };
                let boot_sequence = prev.boot_sequence.wrapping_add(1);
                let reboot = Reboot {
                    reason: classify(evidence),
                    boot_sequence,
                    uptime_before_reboot: prev.last_uptime_secs,
                };
                (
                    BootState {
                        boot_id,
                        boot_sequence,
                        build_id,
                        last_uptime_secs: uptime_secs,
                        saved_at: 0,
                        clean_shutdown: false,
                    },
                    Some(reboot),
                )
            }
            // First run on this device: nothing to compare with.
            None => (
                BootState {
                    boot_id,
                    boot_sequence: 0,
                    build_id,
                    last_uptime_secs: uptime_secs,
                    saved_at: 0,
                    clean_shutdown: false,
                },
                None,
            ),
        };
        let mut tracker = Self { path, state };
        tracker.save()?;
        Ok((tracker, reboot))
    }

    pub fn record_uptime(&mut self, uptime_secs: u32) -> Result<()> {
        self.state.last_uptime_secs = uptime_secs;
        self.save()
    }

    pub fn record_shutdown(&mut self, uptime_secs: u32, system_stopping: bool) -> Result<()> {
        self.state.last_uptime_secs = uptime_secs;
        self.state.clean_shutdown = system_stopping;
        self.save()
    }

    fn save(&mut self) -> Result<()> {
        self.state.saved_at = now_unix();
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&self.state)?)
            .with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A kernel panic/oops record newer than `since` (unix seconds): either still
/// in `/sys/fs/pstore` or already archived by systemd-pstore. Reading pstore
/// usually needs root; without access this reports `false`.
fn panic_record_since(cfg: &Config, since: u64) -> bool {
    let raw = cfg.host_path("/sys/fs/pstore");
    let archived = cfg.host_path("/var/lib/systemd/pstore");
    has_entry_newer_than(&raw, since, |name| name.starts_with("dmesg-"))
        || has_entry_newer_than(&archived, since, |_| true)
}

fn has_entry_newer_than(dir: &Path, since: u64, accept: impl Fn(&str) -> bool) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|e| {
        accept(&e.file_name().to_string_lossy())
            && e.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .is_some_and(|d| d.as_secs() >= since)
    })
}

fn watchdog_reset(cfg: &Config) -> bool {
    let Ok(entries) = std::fs::read_dir(cfg.host_path("/sys/class/watchdog")) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|e| {
        std::fs::read_to_string(e.path().join("bootstatus"))
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .is_some_and(|v| v & WDIOF_CARDRESET != 0)
    })
}

/// True while systemd is shutting the system down.
pub fn system_stopping() -> bool {
    std::process::Command::new("systemctl")
        .arg("is-system-running")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "stopping")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        dir: tempfile::TempDir,
        cfg: Config,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let mut cfg = Config::parse(&format!(
                "server_url = \"http://x\"\nstate_dir = \"{}\"",
                dir.path().join("state").display()
            ))
            .unwrap();
            cfg.root = dir.path().join("root");
            let f = Self { dir, cfg };
            f.set_boot("boot-a");
            f
        }
        fn write(&self, rel: &str, content: &str) {
            let p = self.dir.path().join("root").join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, content).unwrap();
        }
        fn set_boot(&self, id: &str) {
            self.write("proc/sys/kernel/random/boot_id", &format!("{id}\n"));
        }
    }

    #[test]
    fn first_run_and_agent_restart_report_nothing() {
        let f = Fixture::new();
        let (_, reboot) = BootTracker::start(&f.cfg, 1, 10).unwrap();
        assert_eq!(reboot, None);
        let (t, reboot) = BootTracker::start(&f.cfg, 1, 20).unwrap();
        assert_eq!(reboot, None);
        assert_eq!(t.state.boot_sequence, 0);
    }

    #[test]
    fn unclean_reboot_is_unknown_with_last_uptime() {
        let f = Fixture::new();
        let (mut t, _) = BootTracker::start(&f.cfg, 1, 10).unwrap();
        t.record_uptime(3600).unwrap();
        f.set_boot("boot-b");
        let (t, reboot) = BootTracker::start(&f.cfg, 1, 5).unwrap();
        assert_eq!(
            reboot,
            Some(Reboot {
                reason: RebootReason::Unknown,
                boot_sequence: 1,
                uptime_before_reboot: 3600
            })
        );
        assert_eq!(t.state.boot_id, "boot-b");
        assert!(!t.state.clean_shutdown);
    }

    #[test]
    fn clean_shutdown_then_image_update() {
        let f = Fixture::new();
        let (mut t, _) = BootTracker::start(&f.cfg, 1, 10).unwrap();
        t.record_shutdown(100, true).unwrap();
        f.set_boot("boot-b");
        let (mut t, reboot) = BootTracker::start(&f.cfg, 1, 5).unwrap();
        assert_eq!(reboot.unwrap().reason, RebootReason::SoftwareReset);

        t.record_shutdown(50, true).unwrap();
        f.set_boot("boot-c");
        let (_, reboot) = BootTracker::start(&f.cfg, 2, 5).unwrap();
        let reboot = reboot.unwrap();
        assert_eq!(reboot.reason, RebootReason::FirmwareUpdate);
        assert_eq!(reboot.boot_sequence, 2);
    }

    #[test]
    fn agent_stop_without_system_shutdown_is_not_clean() {
        let f = Fixture::new();
        let (mut t, _) = BootTracker::start(&f.cfg, 1, 10).unwrap();
        t.record_shutdown(100, false).unwrap();
        f.set_boot("boot-b");
        let (_, reboot) = BootTracker::start(&f.cfg, 1, 5).unwrap();
        assert_eq!(reboot.unwrap().reason, RebootReason::Unknown);
    }

    #[test]
    fn watchdog_and_panic_records() {
        let f = Fixture::new();
        BootTracker::start(&f.cfg, 1, 10).unwrap();
        f.write("sys/class/watchdog/watchdog0/bootstatus", "32\n");
        f.set_boot("boot-b");
        let (_, reboot) = BootTracker::start(&f.cfg, 1, 5).unwrap();
        assert_eq!(reboot.unwrap().reason, RebootReason::WatchdogTimeout);

        f.write("sys/fs/pstore/dmesg-ramoops-0", "Kernel panic");
        f.set_boot("boot-c");
        let (_, reboot) = BootTracker::start(&f.cfg, 1, 5).unwrap();
        assert_eq!(reboot.unwrap().reason, RebootReason::HardFault);
    }

    #[test]
    fn classification_precedence() {
        let all = Evidence {
            panic_record: true,
            watchdog_reset: true,
            build_changed: true,
            clean_shutdown: true,
        };
        assert_eq!(classify(all), RebootReason::HardFault);
        assert_eq!(classify(Evidence::default()), RebootReason::Unknown);
    }
}
