//! Agent configuration, read from `/etc/ferrite/agent.conf` (TOML).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use ferrite_sdk::encryption::DeviceCredential;
use serde::Deserialize;

/// Longest metric key the chunk format carries.
pub const MAX_METRIC_KEY: usize = 32;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// ferrite-server (or gateway) base URL, e.g. `https://ferrite.example.com`.
    pub server_url: String,
    /// Sent as `X-API-Key` when the server sets `INGEST_API_KEY`.
    #[serde(default)]
    pub api_key: Option<String>,
    /// This device's own key, from `POST /devices/:id/credential`. When set,
    /// every report is sealed with it: the server then knows the report came
    /// from this device, not from anything else holding the API key.
    #[serde(default)]
    pub credential: Option<CredentialConfig>,
    /// Overrides the derived device ID (see [`crate::identity`]).
    #[serde(default)]
    pub device_id: Option<String>,
    /// Overrides the image version read from os-release.
    #[serde(default)]
    pub firmware_version: Option<String>,
    /// Overrides the build ID derived from os-release.
    #[serde(default)]
    pub build_id: Option<u64>,
    /// Seconds between reports.
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    /// Mount points reported as `disk_used_pct` (first) and
    /// `disk_<name>_used_pct` (others, named by their last path component).
    #[serde(default = "default_disks")]
    pub disks: Vec<PathBuf>,
    /// systemd units to watch, e.g. the CODESYS Control runtime.
    #[serde(default)]
    pub units: Vec<UnitConfig>,
    /// Boot tracking and the offline spool live here.
    #[serde(default = "default_state_dir")]
    pub state_dir: PathBuf,
    /// Reports kept while the server is unreachable; the oldest are dropped.
    #[serde(default = "default_max_spooled")]
    pub max_spooled_reports: usize,
    /// HTTP timeout in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Root for `/proc`, `/sys`, `/etc` lookups (tests only).
    #[serde(default = "default_root", skip)]
    pub root: PathBuf,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialConfig {
    pub key_id: u32,
    /// 32 hex characters.
    pub key: String,
}

impl CredentialConfig {
    pub fn device_credential(&self) -> Result<DeviceCredential> {
        let bytes = hex::decode(self.key.trim()).context("credential.key is not hex")?;
        let key: [u8; 16] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("credential.key must be 16 bytes (32 hex characters)"))?;
        if self.key_id == 0 {
            bail!("credential.key_id must not be 0");
        }
        Ok(DeviceCredential {
            key_id: self.key_id,
            key,
        })
    }
}

impl std::fmt::Debug for CredentialConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialConfig")
            .field("key_id", &self.key_id)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitConfig {
    /// Unit name, e.g. `codesyscontrol.service`.
    pub name: String,
    /// Metric prefix; defaults to the unit name without its suffix.
    /// Produces `<key>_active` and `<key>_restarts`.
    #[serde(default)]
    pub key: Option<String>,
}

impl UnitConfig {
    pub fn metric_key(&self) -> String {
        match &self.key {
            Some(k) => k.clone(),
            None => {
                let base = self.name.rsplit_once('.').map_or(&*self.name, |(b, _)| b);
                base.replace(['-', '.', '@'], "_")
            }
        }
    }
}

fn default_interval() -> u64 {
    60
}
fn default_disks() -> Vec<PathBuf> {
    vec![PathBuf::from("/")]
}
fn default_state_dir() -> PathBuf {
    PathBuf::from("/var/lib/ferrite-agent")
}
fn default_max_spooled() -> usize {
    1440
}
fn default_timeout() -> u64 {
    15
}
fn default_root() -> PathBuf {
    PathBuf::from("/")
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let cfg: Config = toml::from_str(text)?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        if !(self.server_url.starts_with("http://") || self.server_url.starts_with("https://")) {
            bail!("server_url must start with http:// or https://");
        }
        if let Some(c) = &self.credential {
            c.device_credential()?;
        }
        if self.interval_secs == 0 {
            bail!("interval_secs must be at least 1");
        }
        for unit in &self.units {
            let longest = format!("{}_restarts", unit.metric_key());
            if longest.len() > MAX_METRIC_KEY {
                bail!(
                    "unit {}: metric key `{longest}` is longer than {MAX_METRIC_KEY} bytes; set a shorter `key`",
                    unit.name
                );
            }
        }
        for disk in self.disks.iter().skip(1) {
            let key = disk_metric_key(disk, false);
            if key.len() > MAX_METRIC_KEY {
                bail!("disk {}: metric key `{key}` is too long", disk.display());
            }
        }
        Ok(())
    }

    /// Path under the configured root, for `/proc`, `/sys` and `/etc` reads.
    pub fn host_path(&self, absolute: &str) -> PathBuf {
        self.root.join(absolute.trim_start_matches('/'))
    }
}

/// Metric key for a disk: `disk_used_pct` for the first, else `disk_<name>_used_pct`.
pub fn disk_metric_key(mount: &Path, first: bool) -> String {
    if first {
        return "disk_used_pct".into();
    }
    let name = mount
        .file_name()
        .map(|n| n.to_string_lossy().replace(['-', '.'], "_"))
        .unwrap_or_else(|| "root".into());
    format!("disk_{name}_used_pct")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_config_gets_defaults() {
        let cfg = Config::parse(r#"server_url = "http://gw:4000""#).unwrap();
        assert_eq!(cfg.interval_secs, 60);
        assert_eq!(cfg.disks, vec![PathBuf::from("/")]);
        assert_eq!(cfg.state_dir, PathBuf::from("/var/lib/ferrite-agent"));
        assert!(cfg.units.is_empty());
    }

    #[test]
    fn unit_keys_default_from_name_and_are_length_checked() {
        let cfg = Config::parse(
            r#"
            server_url = "https://ferrite.example.com"
            [[units]]
            name = "codesyscontrol.service"
            [[units]]
            name = "my-app@1.service"
            key = "app1"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.units[0].metric_key(), "codesyscontrol");
        assert_eq!(cfg.units[1].metric_key(), "app1");

        let err = Config::parse(
            r#"
            server_url = "http://x"
            [[units]]
            name = "a-really-long-industrial-runtime-name.service"
            "#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("set a shorter `key`"), "{err}");
    }

    #[test]
    fn rejects_bad_url_and_unknown_fields() {
        assert!(Config::parse(r#"server_url = "gw:4000""#).is_err());
        assert!(Config::parse("server_url = \"http://x\"\nintervall_secs = 5").is_err());
    }

    #[test]
    fn disk_keys() {
        assert_eq!(disk_metric_key(Path::new("/"), true), "disk_used_pct");
        assert_eq!(
            disk_metric_key(Path::new("/var/lib/codesys"), false),
            "disk_codesys_used_pct"
        );
    }
}
