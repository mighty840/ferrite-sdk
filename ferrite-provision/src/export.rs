use crate::client::BulkEntry;
use anyhow::{Context, Result};
use std::path::Path;

/// Export a single provisioning record to CSV (append mode).
pub fn append_csv(
    path: &Path,
    device_key: u32,
    name: Option<&str>,
    tags: Option<&str>,
) -> Result<()> {
    let exists = path.exists();
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .context("failed to open CSV file")?;
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(!exists)
        .from_writer(file);

    if !exists {
        wtr.write_record(["device_key", "name", "tags"])?;
    }
    wtr.write_record([
        &format!("{:08X}", device_key),
        name.unwrap_or(""),
        tags.unwrap_or(""),
    ])?;
    wtr.flush()?;
    Ok(())
}

/// Import devices from a CSV file.
/// Expected columns: device_key, name (optional), tags (optional), provisioned_by (optional)
pub fn import_csv(path: &Path) -> Result<Vec<BulkEntry>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .context("failed to open CSV file")?;

    let mut entries = Vec::new();
    for result in rdr.records() {
        let record = result.context("failed to read CSV record")?;
        let device_key = record.get(0).unwrap_or("").to_string();
        if device_key.is_empty() {
            continue;
        }
        entries.push(BulkEntry {
            device_key,
            name: record.get(1).and_then(|s| {
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            }),
            tags: record.get(2).and_then(|s| {
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            }),
            provisioned_by: record.get(3).and_then(|s| {
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            }),
        });
    }
    Ok(entries)
}

/// Import devices from a JSON file. Expects an array of BulkEntry objects.
pub fn import_json(path: &Path) -> Result<Vec<BulkEntry>> {
    let data = std::fs::read_to_string(path).context("failed to read JSON file")?;
    let entries: Vec<BulkEntry> = serde_json::from_str(&data).context("failed to parse JSON")?;
    Ok(entries)
}
