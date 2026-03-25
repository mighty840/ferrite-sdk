pub mod devices;
pub mod faults;
pub mod groups;
pub mod metrics;
pub mod ota;

use chrono::Utc;

/// Convert a human duration string (e.g. "1h", "30m") to an ISO 8601 datetime
/// representing `now - duration`.
pub fn duration_to_since(dur_str: &str) -> anyhow::Result<String> {
    let duration = humantime::parse_duration(dur_str)?;
    let since = Utc::now() - chrono::Duration::from_std(duration)?;
    Ok(since.to_rfc3339())
}
