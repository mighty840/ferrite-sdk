use anyhow::Result;
use clap::ValueEnum;
use serde::Serialize;
use tabled::{settings::Style, Table, Tabled};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Csv,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::Table
    }
}

impl OutputFormat {
    /// Parse from an optional string (from config file).
    pub fn from_str_opt(s: Option<&str>) -> Self {
        match s.map(|s| s.to_lowercase()).as_deref() {
            Some("json") => Self::Json,
            Some("csv") => Self::Csv,
            _ => Self::Table,
        }
    }
}

pub fn print_items<T: Serialize + Tabled>(items: &[T], format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Table => {
            if items.is_empty() {
                println!("(no results)");
            } else {
                let table = Table::new(items).with(Style::modern_rounded()).to_string();
                println!("{table}");
            }
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(items)?;
            println!("{json}");
        }
        OutputFormat::Csv => {
            if items.is_empty() {
                return Ok(());
            }
            // Use serde_json to get field names from first item, then print rows
            let values: Vec<serde_json::Value> = items
                .iter()
                .map(|item| serde_json::to_value(item))
                .collect::<Result<_, _>>()?;
            if let Some(serde_json::Value::Object(first)) = values.first() {
                let headers: Vec<&str> = first.keys().map(|k| k.as_str()).collect();
                println!("{}", headers.join(","));
                for val in &values {
                    if let serde_json::Value::Object(map) = val {
                        let row: Vec<String> = headers
                            .iter()
                            .map(|h| {
                                map.get(*h)
                                    .map(|v| match v {
                                        serde_json::Value::String(s) => s.clone(),
                                        serde_json::Value::Null => String::new(),
                                        other => other.to_string(),
                                    })
                                    .unwrap_or_default()
                            })
                            .collect();
                        println!("{}", row.join(","));
                    }
                }
            }
        }
    }
    Ok(())
}
