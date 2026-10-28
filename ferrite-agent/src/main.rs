use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use ferrite_agent::boot::{self, BootTracker};
use ferrite_agent::config::Config;
use ferrite_agent::identity::Identity;
use ferrite_agent::report::{self, Report};
use ferrite_agent::services;
use ferrite_agent::system::{self, SystemSampler};
use ferrite_agent::uplink::{self, Delivery, HttpTransport, Spool};
use ferrite_sdk::chunks::ChunkEncoder;
use ferrite_sdk::encryption::ChunkSealer;

/// Persist the uptime for reboot reports at most this often (flash wear).
const UPTIME_SAVE_INTERVAL_SECS: u32 = 600;

#[derive(Parser)]
#[command(version, about = "Report this Linux device to ferrite-server")]
struct Cli {
    /// Configuration file
    #[arg(long, short, default_value = "/etc/ferrite/agent.conf")]
    config: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Report every `interval_secs` until stopped (default)
    Run,
    /// Send one report and exit; non-zero if it had to be spooled
    Once,
    /// Print the device ID, firmware version and build ID the agent reports
    Identity,
}

fn main() -> Result<()> {
    // RUST_LOG takes a single level (error, warn, info, debug, trace).
    let level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|l| l.parse().ok())
        .unwrap_or(tracing::Level::INFO);
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .init();

    let cli = Cli::parse();
    let cfg = Config::load(&cli.config)?;
    let identity = Identity::resolve(&cfg)?;

    match cli.command.unwrap_or(Command::Run) {
        Command::Identity => {
            println!("{}", serde_json::to_string_pretty(&identity)?);
            Ok(())
        }
        Command::Once => {
            let mut agent = Agent::new(cfg, identity)?;
            // CPU usage needs two readings.
            std::thread::sleep(Duration::from_secs(1));
            match agent.tick()? {
                Delivery::Accepted => Ok(()),
                Delivery::Failed => bail!("report not delivered; spooled for the next run"),
                Delivery::Rejected(status) => bail!("server rejected the report (HTTP {status})"),
            }
        }
        Command::Run => {
            let stop = Arc::new(AtomicBool::new(false));
            signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
            signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
            let mut agent = Agent::new(cfg, identity)?;
            agent.run(&stop)
        }
    }
}

struct Agent {
    cfg: Config,
    identity: Identity,
    encoder: ChunkEncoder,
    sampler: SystemSampler,
    tracker: BootTracker,
    pending_reboot: Option<boot::Reboot>,
    transport: HttpTransport,
    /// Seals every chunk with the device credential, when configured.
    sealer: Option<ChunkSealer>,
    spool: Spool,
    uptime_saved: u32,
}

impl Agent {
    fn new(cfg: Config, identity: Identity) -> Result<Self> {
        let uptime = uptime_secs(&cfg);
        let (tracker, pending_reboot) = BootTracker::start(&cfg, identity.build_id, uptime)?;
        tracing::info!(
            device_id = %identity.device_id,
            firmware_version = %identity.firmware_version,
            build_id = identity.build_id,
            reboot = ?pending_reboot.map(|r| r.reason),
            "ferrite-agent starting"
        );
        let transport = HttpTransport::new(
            &cfg.server_url,
            cfg.api_key.clone(),
            Duration::from_secs(cfg.timeout_secs),
        )?;
        let spool = Spool::open(&cfg.state_dir.join("spool"), cfg.max_spooled_reports)?;
        // A fresh random epoch per start keeps nonces unique: the counter
        // restarts with the process.
        let sealer = match &cfg.credential {
            Some(c) => {
                let credential = c.device_credential()?;
                tracing::info!(
                    key_id = credential.key_id,
                    "sealing reports with the device credential"
                );
                Some(ChunkSealer::for_device(&credential, random_u32()?))
            }
            None => None,
        };
        Ok(Self {
            sealer,
            sampler: SystemSampler::new(&cfg),
            encoder: ChunkEncoder::new(),
            tracker,
            pending_reboot,
            transport,
            spool,
            uptime_saved: uptime,
            identity,
            cfg,
        })
    }

    fn tick(&mut self) -> Result<Delivery> {
        let uptime = uptime_secs(&self.cfg);
        let mut samples = self.sampler.sample(&self.cfg);
        samples.extend(services::sample(&self.cfg));
        let batch = report::encode(
            &mut self.encoder,
            &Report {
                identity: &self.identity,
                uptime_ms: u64::from(uptime) * 1000,
                mem_available_bytes: system::mem_available_bytes(&self.cfg),
                samples: &samples,
                reboot: self.pending_reboot,
            },
            self.sealer.as_mut(),
        );
        // Once encoded, the reboot travels with the batch, spooled or not.
        self.pending_reboot = None;
        let result = uplink::deliver(&self.transport, &mut self.spool, &batch)?;
        tracing::debug!(?result, metrics = samples.len(), "report");
        if uptime.saturating_sub(self.uptime_saved) >= UPTIME_SAVE_INTERVAL_SECS {
            self.tracker.record_uptime(uptime)?;
            self.uptime_saved = uptime;
        }
        Ok(result)
    }

    fn run(&mut self, stop: &AtomicBool) -> Result<()> {
        let interval = Duration::from_secs(self.cfg.interval_secs);
        let mut next = Instant::now() + Duration::from_secs(1);
        while !stop.load(Ordering::Relaxed) {
            if Instant::now() >= next {
                if let Err(e) = self.tick() {
                    tracing::error!(error = %e, "report failed");
                }
                next += interval;
                if next < Instant::now() {
                    next = Instant::now() + interval;
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let stopping = boot::system_stopping();
        tracing::info!(system_shutdown = stopping, "ferrite-agent stopping");
        self.tracker
            .record_shutdown(uptime_secs(&self.cfg), stopping)
    }
}

fn random_u32() -> Result<u32> {
    use std::io::Read;
    let mut b = [0u8; 4];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .context("reading /dev/urandom")?;
    Ok(u32::from_le_bytes(b))
}

fn uptime_secs(cfg: &Config) -> u32 {
    std::fs::read_to_string(cfg.host_path("/proc/uptime"))
        .ok()
        .and_then(|s| system::parse_uptime(&s))
        .map_or(0, |s| s as u32)
}
