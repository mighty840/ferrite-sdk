pub mod devices;
pub mod evidence;
pub mod faults;
pub mod groups;
pub mod metrics;
pub mod ota;
pub mod report;

use chrono::Utc;

/// Convert a human duration string (e.g. "1h", "30m") to the UTC time
/// `now - duration`, formatted like the server's `created_at` columns
/// (`YYYY-MM-DD HH:MM:SS`), which it compares as text.
pub fn duration_to_since(dur_str: &str) -> anyhow::Result<String> {
    let duration = humantime::parse_duration(dur_str)?;
    let since = Utc::now() - chrono::Duration::from_std(duration)?;
    Ok(since.format("%Y-%m-%d %H:%M:%S").to_string())
}

#[cfg(test)]
mod tests {
    use super::duration_to_since;

    #[test]
    fn since_matches_sqlite_datetime_format() {
        let s = duration_to_since("1h").unwrap();
        assert_eq!(s.len(), 19, "{s}");
        assert_eq!(&s[10..11], " ");
        assert!(chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").is_ok());
    }
}
