use anyhow::Result;
use clap::Subcommand;

use crate::api::ApiClient;
use crate::output::{print_items, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum DevicesCommand {
    /// List all known devices
    List,
    /// Show detailed info for a device (by hex device key, e.g. A300F1B2)
    Inspect {
        /// Device key in hex (e.g. A300F1B2)
        device_key: String,
    },
    /// Delete a device by hex device key
    Delete {
        /// Device key in hex (e.g. A300F1B2)
        device_key: String,
    },
}

pub fn run(cmd: &DevicesCommand, client: &ApiClient, format: OutputFormat) -> Result<()> {
    match cmd {
        DevicesCommand::List => {
            let devices = client.list_devices()?;
            print_items(&devices, format)?;
        }
        DevicesCommand::Inspect { device_key } => {
            let device = client.get_device(device_key)?;
            println!("=== Device ===");
            print_items(&[device.clone()], format)?;

            // Fetch recent faults for this device
            let faults = client.list_device_faults(&device.device_id, None)?;
            println!("\n=== Recent Faults ({}) ===", faults.len());
            print_items(&faults, format)?;

            // Fetch recent metrics for this device
            let metrics = client.list_device_metrics(&device.device_id, None)?;
            println!("\n=== Recent Metrics ({}) ===", metrics.len());
            print_items(&metrics, format)?;
        }
        DevicesCommand::Delete { device_key } => {
            client.delete_device(device_key)?;
            println!("Device {device_key} deleted.");
        }
    }
    Ok(())
}
