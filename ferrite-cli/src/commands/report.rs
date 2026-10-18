use anyhow::Result;
use clap::Subcommand;

use crate::api::{ApiClient, ReachDevice};
use crate::output::{print_items, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum ReportCommand {
    /// Devices per firmware build
    Builds {
        /// Only devices in this group
        #[arg(long)]
        group: Option<i64>,
        /// Only devices with this tag
        #[arg(long)]
        tag: Option<String>,
    },
    /// Update reach of an OTA campaign: who runs the target build, who is
    /// unreachable, time to patch
    Campaign {
        /// Campaign ID
        campaign_id: i64,
    },
    /// Every build a device has reported
    History {
        /// Device ID string
        device_id: String,
    },
}

pub fn run(cmd: &ReportCommand, client: &ApiClient, format: OutputFormat) -> Result<()> {
    match cmd {
        ReportCommand::Builds { group, tag } => {
            let builds = client.report_builds(*group, tag.as_deref())?;
            print_items(&builds, format)?;
        }
        ReportCommand::Campaign { campaign_id } => {
            let reach = client.report_campaign(*campaign_id)?;
            if let OutputFormat::Json = format {
                println!("{}", serde_json::to_string_pretty(&reach)?);
                return Ok(());
            }
            let devices: Vec<ReachDevice> = serde_json::from_value(reach["devices"].clone())?;
            if let OutputFormat::Table = format {
                println!("{}", summary_line(&reach));
            }
            print_items(&devices, format)?;
        }
        ReportCommand::History { device_id } => {
            let history = client.report_firmware_history(device_id)?;
            print_items(&history, format)?;
        }
    }
    Ok(())
}

fn summary_line(r: &serde_json::Value) -> String {
    let ttp = &r["time_to_patch"];
    let median = ttp["median_secs"]
        .as_i64()
        .map(|s| {
            humantime::format_duration(std::time::Duration::from_secs(s.max(0) as u64)).to_string()
        })
        .unwrap_or_else(|| "-".into());
    format!(
        "Campaign #{} \"{}\" ({}): target {} build {} — {}/{} on target ({}%), {} failed, {} pending, {} unreachable; median time to patch {}",
        r["campaign_id"],
        r["name"].as_str().unwrap_or(""),
        r["status"].as_str().unwrap_or(""),
        r["target_version"].as_str().unwrap_or(""),
        r["target_build_id"],
        r["on_target"],
        r["targeted"],
        r["percent_on_target"],
        r["failed"],
        r["pending"],
        r["unreachable"],
        median,
    )
}
