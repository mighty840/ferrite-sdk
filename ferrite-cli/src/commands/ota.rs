use anyhow::Result;
use clap::Subcommand;

use crate::api::ApiClient;
use crate::output::{print_items, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum OtaCommand {
    /// Deploy an OTA target for a device
    Deploy {
        /// Device ID string
        device_id: String,
        /// Target firmware version
        #[arg(long)]
        version: String,
        /// Target build ID
        #[arg(long, default_value = "0")]
        build_id: i64,
        /// Optional firmware download URL
        #[arg(long)]
        firmware_url: Option<String>,
    },
    /// Show OTA target status for a device
    Status {
        /// Device ID string
        device_id: String,
    },
    /// List all OTA targets
    List,
    /// Cancel (remove) OTA target for a device
    Cancel {
        /// Device ID string
        device_id: String,
    },
}

pub fn run(cmd: &OtaCommand, client: &ApiClient, format: OutputFormat) -> Result<()> {
    match cmd {
        OtaCommand::Deploy {
            device_id,
            version,
            build_id,
            firmware_url,
        } => {
            let target =
                client.set_ota_target(device_id, version, *build_id, firmware_url.as_deref())?;
            println!("OTA target set for {device_id}: {}", target.target_version);
        }
        OtaCommand::Status { device_id } => {
            let target = client.get_ota_target(device_id)?;
            print_items(&[target], format)?;
        }
        OtaCommand::List => {
            let targets = client.list_ota_targets()?;
            print_items(&targets, format)?;
        }
        OtaCommand::Cancel { device_id } => {
            client.delete_ota_target(device_id)?;
            println!("OTA target cancelled for {device_id}.");
        }
    }
    Ok(())
}
