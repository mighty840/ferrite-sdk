//! System metrics from `/proc` and `/sys`.
//!
//! The chunk format carries `Counter(u32)` and `Gauge(f32)`, so sizes are
//! reported in MiB or percent, never bytes.

use std::path::Path;

use crate::config::{disk_metric_key, Config};

/// One metric to report.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Gauge(f32),
    Counter(u32),
}

pub type Sample = (String, Value);

/// CPU time counters from the first line of `/proc/stat`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

pub fn parse_cpu_times(stat: &str) -> Option<CpuTimes> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|f| f.parse().ok())
        .collect();
    if fields.len() < 4 {
        return None;
    }
    // user nice system idle iowait irq softirq steal (guest is inside user)
    let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
    let total: u64 = fields.iter().take(8).sum();
    Some(CpuTimes {
        busy: total - idle,
        total,
    })
}

/// CPU busy percentage between two samples.
pub fn cpu_pct(prev: CpuTimes, now: CpuTimes) -> Option<f32> {
    let total = now.total.checked_sub(prev.total)?;
    let busy = now.busy.checked_sub(prev.busy)?;
    if total == 0 {
        return None;
    }
    Some((busy as f64 * 100.0 / total as f64) as f32)
}

/// `(used_pct, available_mib)` from `/proc/meminfo`.
pub fn parse_meminfo(text: &str) -> Option<(f32, f32)> {
    let get = |key: &str| -> Option<u64> {
        text.lines()
            .find(|l| l.starts_with(key) && l[key.len()..].starts_with(':'))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    };
    let total = get("MemTotal")?;
    let available = get("MemAvailable").or_else(|| {
        Some(get("MemFree")? + get("Buffers").unwrap_or(0) + get("Cached").unwrap_or(0))
    })?;
    if total == 0 {
        return None;
    }
    let used_pct = (total.saturating_sub(available)) as f64 * 100.0 / total as f64;
    Some((used_pct as f32, available as f32 / 1024.0))
}

/// Seconds since boot from `/proc/uptime`.
pub fn parse_uptime(text: &str) -> Option<f64> {
    text.split_whitespace().next()?.parse().ok()
}

/// 1-minute load average from `/proc/loadavg`.
pub fn parse_loadavg(text: &str) -> Option<f32> {
    text.split_whitespace().next()?.parse().ok()
}

/// Hottest thermal zone in °C, from `/sys/class/thermal/thermal_zone*/temp`
/// (millidegrees).
pub fn max_temperature(thermal_dir: &Path) -> Option<f32> {
    let entries = std::fs::read_dir(thermal_dir).ok()?;
    entries
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("thermal_zone"))
        .filter_map(|e| std::fs::read_to_string(e.path().join("temp")).ok())
        .filter_map(|t| t.trim().parse::<i64>().ok())
        .map(|milli| milli as f32 / 1000.0)
        .fold(None, |max: Option<f32>, t| {
            Some(max.map_or(t, |m| m.max(t)))
        })
}

/// Used percentage of the filesystem holding `path`.
pub fn disk_used_pct(path: &Path) -> Option<f32> {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs only writes into the zeroed struct we pass.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let total = st.f_blocks as f64;
    // Like df: space reserved for root counts as neither used nor available.
    let avail = st.f_bavail as f64;
    let used = total - st.f_bfree as f64;
    if used + avail <= 0.0 {
        return None;
    }
    Some((used * 100.0 / (used + avail)) as f32)
}

/// Samples system metrics; keeps the previous CPU reading between calls.
pub struct SystemSampler {
    prev_cpu: Option<CpuTimes>,
}

impl SystemSampler {
    pub fn new(cfg: &Config) -> Self {
        let prev_cpu = std::fs::read_to_string(cfg.host_path("/proc/stat"))
            .ok()
            .and_then(|s| parse_cpu_times(&s));
        Self { prev_cpu }
    }

    pub fn sample(&mut self, cfg: &Config) -> Vec<Sample> {
        let read = |p: &str| std::fs::read_to_string(cfg.host_path(p)).ok();
        let mut out = Vec::new();

        if let Some(up) = read("/proc/uptime").and_then(|s| parse_uptime(&s)) {
            out.push(("uptime_seconds".into(), Value::Gauge(up as f32)));
        }
        if let Some(now) = read("/proc/stat").and_then(|s| parse_cpu_times(&s)) {
            if let Some(pct) = self.prev_cpu.and_then(|prev| cpu_pct(prev, now)) {
                out.push(("cpu_pct".into(), Value::Gauge(pct)));
            }
            self.prev_cpu = Some(now);
        }
        if let Some(load) = read("/proc/loadavg").and_then(|s| parse_loadavg(&s)) {
            out.push(("load1".into(), Value::Gauge(load)));
        }
        if let Some((used, avail)) = read("/proc/meminfo").and_then(|s| parse_meminfo(&s)) {
            out.push(("mem_used_pct".into(), Value::Gauge(used)));
            out.push(("mem_avail_mib".into(), Value::Gauge(avail)));
        }
        if let Some(t) = max_temperature(&cfg.host_path("/sys/class/thermal")) {
            out.push(("cpu_temp".into(), Value::Gauge(t)));
        }
        for (i, disk) in cfg.disks.iter().enumerate() {
            if let Some(pct) = disk_used_pct(disk) {
                out.push((disk_metric_key(disk, i == 0), Value::Gauge(pct)));
            }
        }
        out
    }
}

/// Available memory in bytes, saturating at `u32::MAX`. The heartbeat's
/// `free_stack_bytes` field carries it, as stack has no meaning on Linux.
pub fn mem_available_bytes(cfg: &Config) -> u32 {
    std::fs::read_to_string(cfg.host_path("/proc/meminfo"))
        .ok()
        .and_then(|s| parse_meminfo(&s))
        .map_or(0, |(_, mib)| {
            (mib as f64 * 1024.0 * 1024.0).min(u32::MAX as f64) as u32
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_percentage_between_samples() {
        let a = parse_cpu_times("cpu  100 0 100 700 100 0 0 0 0 0\ncpu0 1 2 3 4\n").unwrap();
        let b = parse_cpu_times("cpu  150 0 150 750 150 0 0 0 0 0\n").unwrap();
        // busy +100, total +200
        assert_eq!(cpu_pct(a, b), Some(50.0));
        assert_eq!(cpu_pct(b, a), None);
        assert_eq!(cpu_pct(a, a), None);
    }

    #[test]
    fn meminfo_prefers_available() {
        let text = "MemTotal:        1024000 kB\nMemFree:          100000 kB\nMemAvailable:     512000 kB\n";
        let (used, avail) = parse_meminfo(text).unwrap();
        assert_eq!(used, 50.0);
        assert_eq!(avail, 500.0);
        // Old kernels without MemAvailable.
        let old = "MemTotal: 1000 kB\nMemFree: 200 kB\nBuffers: 100 kB\nCached: 200 kB\n";
        assert_eq!(parse_meminfo(old).unwrap().0, 50.0);
        // "MemTotalX" must not match "MemTotal".
        assert!(parse_meminfo("MemTotalX: 5 kB\n").is_none());
    }

    #[test]
    fn uptime_and_load() {
        assert_eq!(parse_uptime("12345.67 2345.00\n"), Some(12345.67));
        assert_eq!(parse_loadavg("0.52 0.58 0.59 1/389 12345\n"), Some(0.52));
    }

    #[test]
    fn hottest_thermal_zone() {
        let dir = tempfile::tempdir().unwrap();
        for (zone, milli) in [("thermal_zone0", "45500"), ("thermal_zone1", "61000")] {
            std::fs::create_dir_all(dir.path().join(zone)).unwrap();
            std::fs::write(dir.path().join(zone).join("temp"), milli).unwrap();
        }
        std::fs::create_dir_all(dir.path().join("cooling_device0")).unwrap();
        assert_eq!(max_temperature(dir.path()), Some(61.0));
        assert_eq!(max_temperature(&dir.path().join("missing")), None);
    }

    #[test]
    fn disk_usage_of_tmp_is_a_percentage() {
        let pct = disk_used_pct(Path::new("/tmp")).unwrap();
        assert!((0.0..=100.0).contains(&pct), "{pct}");
        assert!(disk_used_pct(Path::new("/definitely/not/here")).is_none());
    }

    #[test]
    fn sampler_reads_fixture_root() {
        let dir = tempfile::tempdir().unwrap();
        let w = |p: &str, s: &str| {
            let path = dir.path().join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, s).unwrap();
        };
        w("proc/stat", "cpu  100 0 100 800 0 0 0 0\n");
        w("proc/uptime", "60.00 50.00\n");
        w("proc/loadavg", "1.50 1.00 0.50 1/100 1\n");
        w(
            "proc/meminfo",
            "MemTotal: 2048 kB\nMemFree: 0 kB\nMemAvailable: 1024 kB\n",
        );
        w("sys/class/thermal/thermal_zone0/temp", "50000\n");
        let mut cfg = Config::parse("server_url = \"http://x\"\ndisks = [\"/tmp\"]").unwrap();
        cfg.root = dir.path().to_path_buf();

        let mut sampler = SystemSampler::new(&cfg);
        w("proc/stat", "cpu  200 0 100 900 0 0 0 0\n");
        let samples = sampler.sample(&cfg);
        let get = |k: &str| {
            samples
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(get("uptime_seconds"), Some(Value::Gauge(60.0)));
        assert_eq!(get("cpu_pct"), Some(Value::Gauge(50.0)));
        assert_eq!(get("load1"), Some(Value::Gauge(1.5)));
        assert_eq!(get("mem_used_pct"), Some(Value::Gauge(50.0)));
        assert_eq!(get("mem_avail_mib"), Some(Value::Gauge(1.0)));
        assert_eq!(get("cpu_temp"), Some(Value::Gauge(50.0)));
        assert!(get("disk_used_pct").is_some());
        assert_eq!(mem_available_bytes(&cfg), 1024 * 1024);
    }
}
