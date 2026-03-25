use anyhow::Result;
use clap::Args;

use super::duration_to_since;
use crate::api::ApiClient;
use crate::output::{print_items, OutputFormat};

#[derive(Debug, Args)]
pub struct FaultsArgs {
    /// Filter by device_id
    #[arg(long)]
    device: Option<String>,

    /// Show faults since duration (e.g. "1h", "30m", "7d")
    #[arg(long)]
    since: Option<String>,
}

pub fn run(args: &FaultsArgs, client: &ApiClient, format: OutputFormat) -> Result<()> {
    let since = args.since.as_deref().map(duration_to_since).transpose()?;

    let faults = if let Some(device_id) = &args.device {
        client.list_device_faults(device_id, since.as_deref())?
    } else {
        client.list_all_faults(since.as_deref())?
    };

    print_items(&faults, format)?;
    Ok(())
}
