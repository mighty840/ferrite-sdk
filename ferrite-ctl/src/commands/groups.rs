use anyhow::Result;
use clap::Subcommand;

use crate::api::ApiClient;
use crate::output::{print_items, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum GroupsCommand {
    /// List all device groups
    List,
    /// Create a new device group
    Create {
        /// Group name
        name: String,
        /// Optional description
        #[arg(long)]
        description: Option<String>,
    },
    /// Add a device to a group
    AddDevice {
        /// Group ID
        group_id: i64,
        /// Device ID string
        device_id: String,
    },
    /// Remove a device from a group
    RemoveDevice {
        /// Group ID
        group_id: i64,
        /// Device ID string
        device_id: String,
    },
    /// List devices in a group
    Devices {
        /// Group ID
        group_id: i64,
    },
}

pub fn run(cmd: &GroupsCommand, client: &ApiClient, format: OutputFormat) -> Result<()> {
    match cmd {
        GroupsCommand::List => {
            let groups = client.list_groups()?;
            print_items(&groups, format)?;
        }
        GroupsCommand::Create { name, description } => {
            let group = client.create_group(name, description.as_deref())?;
            println!("Created group #{}: {}", group.id, group.name);
        }
        GroupsCommand::AddDevice {
            group_id,
            device_id,
        } => {
            client.add_device_to_group(*group_id, device_id)?;
            println!("Added device {device_id} to group {group_id}.");
        }
        GroupsCommand::RemoveDevice {
            group_id,
            device_id,
        } => {
            client.remove_device_from_group(*group_id, device_id)?;
            println!("Removed device {device_id} from group {group_id}.");
        }
        GroupsCommand::Devices { group_id } => {
            let devices = client.list_group_devices(*group_id)?;
            print_items(&devices, format)?;
        }
    }
    Ok(())
}
