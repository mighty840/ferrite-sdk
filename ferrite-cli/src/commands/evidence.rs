use anyhow::{bail, Result};
use clap::Subcommand;

use super::duration_to_since;
use crate::api::ApiClient;
use crate::output::{print_items, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum EvidenceCommand {
    /// List evidence log entries (oldest first)
    List {
        /// Entries since duration (e.g. "24h", "7d"); overrides --from
        #[arg(long)]
        since: Option<String>,
        /// Entries at or after this time (e.g. "2026-10-01" or RFC 3339)
        #[arg(long)]
        from: Option<String>,
        /// Entries at or before this time
        #[arg(long)]
        to: Option<String>,
        /// Event kind or kind prefix (e.g. "campaign" or "device.update_installed")
        #[arg(long)]
        kind: Option<String>,
        /// Subject (e.g. "device:dev-1", "campaign:3", "firmware:7")
        #[arg(long)]
        subject: Option<String>,
        /// Maximum number of entries
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Recompute the server's hash chain and report whether it is intact
    Verify,
}

pub fn run(cmd: &EvidenceCommand, client: &ApiClient, format: OutputFormat) -> Result<()> {
    match cmd {
        EvidenceCommand::List {
            since,
            from,
            to,
            kind,
            subject,
            limit,
        } => {
            let from = match since {
                Some(d) => Some(duration_to_since(d)?),
                None => from.clone(),
            };
            let limit = limit.map(|l| l.to_string());
            let mut query: Vec<(&str, &str)> = Vec::new();
            for (name, value) in [
                ("from", from.as_deref()),
                ("to", to.as_deref()),
                ("kind", kind.as_deref()),
                ("subject", subject.as_deref()),
                ("limit", limit.as_deref()),
            ] {
                if let Some(v) = value {
                    query.push((name, v));
                }
            }
            let page = client.list_evidence(&query)?;
            print_items(&page.entries, format)?;
            if !page.chain_valid {
                eprintln!("warning: the server reports a BROKEN evidence chain; run `ferrite evidence verify`");
            }
        }
        EvidenceCommand::Verify => {
            let report = client.verify_evidence()?;
            match format {
                OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
                _ => {
                    println!("entries:   {}", report.entries);
                    println!(
                        "head:      {} {}",
                        report.head_seq.map_or("-".into(), |s| s.to_string()),
                        report.head_hash.as_deref().unwrap_or("-")
                    );
                    match report.first_invalid_seq {
                        None => println!("chain:     OK"),
                        Some(seq) => println!("chain:     BROKEN at seq {seq}"),
                    }
                }
            }
            if !report.valid {
                bail!("evidence chain is broken");
            }
        }
    }
    Ok(())
}
