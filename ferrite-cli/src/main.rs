mod api;
mod commands;
mod config;
mod output;

use anyhow::Result;
use clap::{Parser, Subcommand};

use api::ApiClient;
use config::{make_auth_header, CliConfig};
use output::OutputFormat;

#[derive(Parser)]
#[command(name = "ferrite", about = "CLI for the ferrite-server REST API")]
struct Cli {
    /// Server URL (overrides config)
    #[arg(long, global = true)]
    server: Option<String>,

    /// Output format
    #[arg(long, global = true, value_enum)]
    format: Option<OutputFormat>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Save server credentials and validate connectivity
    Login {
        /// Server URL (e.g. http://localhost:4000)
        #[arg(long)]
        server: Option<String>,
        /// Username
        #[arg(long)]
        user: Option<String>,
        /// Password
        #[arg(long)]
        pass: Option<String>,
    },
    /// Manage devices
    Devices {
        #[command(subcommand)]
        cmd: commands::devices::DevicesCommand,
    },
    /// Query fault events
    Faults(commands::faults::FaultsArgs),
    /// Manage device groups
    Groups {
        #[command(subcommand)]
        cmd: commands::groups::GroupsCommand,
    },
    /// Query metrics
    Metrics(commands::metrics::MetricsArgs),
    /// Manage OTA firmware targets
    Ota {
        #[command(subcommand)]
        cmd: commands::ota::OtaCommand,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Login { server, user, pass } => {
            let mut cfg = CliConfig::load()?;

            if let Some(s) = server {
                cfg.server = s.clone();
            }
            if let Some(u) = user {
                cfg.username = u.clone();
            }
            if let Some(p) = pass {
                cfg.password = p.clone();
            }

            // Validate connectivity
            let auth = make_auth_header(&cfg.username, &cfg.password);
            let client = ApiClient::new(&cfg.server, &auth)?;
            client.health()?;

            cfg.save()?;
            println!("Logged in to {} as {}", cfg.server, cfg.username);
        }
        cmd => {
            let cfg = CliConfig::load()?;
            let server = cli.server.as_deref().unwrap_or(&cfg.server);
            let auth = make_auth_header(&cfg.username, &cfg.password);
            let client = ApiClient::new(server, &auth)?;

            let format = cli
                .format
                .unwrap_or_else(|| OutputFormat::from_str_opt(cfg.default_format.as_deref()));

            match cmd {
                Commands::Devices { cmd: subcmd } => {
                    commands::devices::run(subcmd, &client, format)?;
                }
                Commands::Faults(args) => {
                    commands::faults::run(args, &client, format)?;
                }
                Commands::Groups { cmd: subcmd } => {
                    commands::groups::run(subcmd, &client, format)?;
                }
                Commands::Metrics(args) => {
                    commands::metrics::run(args, &client, format)?;
                }
                Commands::Ota { cmd: subcmd } => {
                    commands::ota::run(subcmd, &client, format)?;
                }
                Commands::Login { .. } => unreachable!(),
            }
        }
    }

    Ok(())
}
