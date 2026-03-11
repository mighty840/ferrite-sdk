mod client;
mod export;
mod uart;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "ferrite-provision",
    about = "CLI tool for ferrite device provisioning and registration"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Provision a device key via UART and optionally register with server
    Provision {
        /// Serial port (e.g. /dev/ttyACM0, COM3)
        #[arg(long)]
        port: String,

        /// Baud rate
        #[arg(long, default_value = "115200")]
        baud: u32,

        /// Server URL for registration (e.g. http://localhost:4000)
        #[arg(long)]
        server: Option<String>,

        /// Username for server auth
        #[arg(long)]
        user: Option<String>,

        /// Password for server auth
        #[arg(long)]
        password: Option<String>,

        /// CSV output file (append mode)
        #[arg(long)]
        output: Option<String>,

        /// Human-readable device name
        #[arg(long)]
        name: Option<String>,

        /// Comma-separated tags
        #[arg(long)]
        tags: Option<String>,
    },

    /// Import devices from CSV/JSON and register with server
    Import {
        /// CSV or JSON file path
        #[arg(long)]
        file: String,

        /// Server URL
        #[arg(long)]
        server: String,

        /// Username for server auth
        #[arg(long)]
        user: Option<String>,

        /// Password for server auth
        #[arg(long)]
        password: Option<String>,
    },

    /// Read the device key over UART
    ReadKey {
        /// Serial port
        #[arg(long)]
        port: String,

        /// Baud rate
        #[arg(long, default_value = "115200")]
        baud: u32,
    },

    /// Clear the device key over UART
    ClearKey {
        /// Serial port
        #[arg(long)]
        port: String,

        /// Baud rate
        #[arg(long, default_value = "115200")]
        baud: u32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Provision {
            port,
            baud,
            server,
            user,
            password,
            output,
            name,
            tags,
        } => {
            let mut conn = uart::UartConnection::open(&port, baud)?;

            // Ping device
            if !conn.ping()? {
                bail!("device did not respond to PING on {port}");
            }
            println!("Device connected on {port}");

            // Compute owner prefix from username
            let owner_prefix = match &user {
                Some(u) => client::compute_owner_prefix(u),
                None => 0x00,
            };

            // Generate entropy seed from timestamp
            let entropy_seed = chrono::Utc::now().timestamp() as u32;

            let key = conn.provision(owner_prefix, entropy_seed)?;
            let formatted = uart::format_device_key(key);
            println!("Provisioned device key: {formatted}");

            // Optionally register with server
            if let Some(server_url) = &server {
                let srv =
                    client::ServerClient::new(server_url, user.as_deref(), password.as_deref());
                let resp =
                    srv.register_device(key, name.as_deref(), tags.as_deref(), user.as_deref())?;
                if let Some(err) = resp.error {
                    eprintln!("Server error: {err}");
                } else {
                    println!("Registered with server");
                }
            }

            // Optionally write to CSV
            if let Some(csv_path) = &output {
                export::append_csv(
                    std::path::Path::new(csv_path),
                    key,
                    name.as_deref(),
                    tags.as_deref(),
                )?;
                println!("Appended to {csv_path}");
            }
        }

        Command::Import {
            file,
            server,
            user,
            password,
        } => {
            let path = std::path::Path::new(&file);
            let entries = if file.ends_with(".json") {
                export::import_json(path)?
            } else {
                export::import_csv(path)?
            };

            println!("Loaded {} device(s) from {file}", entries.len());

            let srv = client::ServerClient::new(&server, user.as_deref(), password.as_deref());
            let resp = srv.register_devices_bulk(&entries)?;

            if let Some(count) = resp.registered {
                println!("Registered: {count}");
            }
            if let Some(errors) = &resp.errors {
                for err in errors {
                    eprintln!("Error: {err}");
                }
            }
        }

        Command::ReadKey { port, baud } => {
            let mut conn = uart::UartConnection::open(&port, baud)?;
            match conn.read_key()? {
                Some(key) => println!("Device key: {}", uart::format_device_key(key)),
                None => println!("No device key provisioned"),
            }
        }

        Command::ClearKey { port, baud } => {
            let mut conn = uart::UartConnection::open(&port, baud)?;
            if conn.clear_key()? {
                println!("Device key cleared");
            } else {
                println!("Failed to clear device key");
            }
        }
    }

    Ok(())
}
