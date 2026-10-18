//! Device and image identity.
//!
//! The server knows a device by a string ID, a firmware version string and a
//! numeric `build_id`; OTA campaigns and update-reach reports match on
//! `build_id`. On Linux these come from `/etc/os-release`:
//!
//! - **firmware_version:** `IMAGE_VERSION`, else `VERSION_ID`, else `VERSION`.
//! - **build_id:** from `BUILD_ID`, else `IMAGE_VERSION`, else `VERSION_ID`.
//!   A value that is all decimal digits and fits a `u64` is used as is (Yocto's
//!   default `BUILD_ID` is the `DATETIME` stamp, e.g. `20261018093000`).
//!   Anything else maps to the first 8 bytes of its SHA-256, read as a
//!   little-endian `u64`. `ferrite-agent identity` prints the result.
//! - **device_id:** `linux-` + the first 16 hex digits of
//!   SHA-256(`"ferrite-agent:"` + `/etc/machine-id`). systemd asks
//!   applications not to expose the machine ID itself, and the salt keeps the
//!   ferrite ID unlinkable to other services' IDs.
//!
//! Every value can be overridden in the config file.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::config::Config;

/// The `DeviceInfo` chunk carries at most this many bytes of the ID and of
/// the version.
pub const MAX_ID_LEN: usize = 127;
/// ...and both together must fit one 246-byte payload with their two length
/// bytes and the 8-byte build ID.
const MAX_ID_PLUS_VERSION: usize = 236;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Identity {
    pub device_id: String,
    pub firmware_version: String,
    pub build_id: u64,
    /// The os-release value `build_id` was derived from, for display.
    pub build_id_source: String,
}

impl Identity {
    pub fn resolve(cfg: &Config) -> Result<Self> {
        let os_release = read_os_release(&cfg.host_path("/etc/os-release"))
            .or_else(|_| read_os_release(&cfg.host_path("/usr/lib/os-release")))
            .unwrap_or_default();

        let device_id = match &cfg.device_id {
            Some(id) => id.clone(),
            None => {
                let path = cfg.host_path("/etc/machine-id");
                let machine_id = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading {} (or set device_id)", path.display()))?;
                device_id_from_machine_id(machine_id.trim())?
            }
        };
        if device_id.is_empty() || device_id.len() > MAX_ID_LEN {
            bail!("device_id must be 1..={MAX_ID_LEN} bytes");
        }

        let firmware_version = cfg
            .firmware_version
            .clone()
            .or_else(|| first_of(&os_release, &["IMAGE_VERSION", "VERSION_ID", "VERSION"]))
            .unwrap_or_else(|| "unknown".into());
        let firmware_version = truncate(
            &firmware_version,
            MAX_ID_LEN.min(MAX_ID_PLUS_VERSION - device_id.len()),
        );

        let (build_id, build_id_source) = match cfg.build_id {
            Some(id) => (id, "config".to_string()),
            None => {
                let source = first_of(&os_release, &["BUILD_ID", "IMAGE_VERSION", "VERSION_ID"])
                    .unwrap_or_default();
                (build_id_from_str(&source), source)
            }
        };

        Ok(Self {
            device_id,
            firmware_version,
            build_id,
            build_id_source,
        })
    }
}

/// Parse an os-release file (`KEY=value`, values optionally quoted).
pub fn read_os_release(path: &Path) -> Result<HashMap<String, String>> {
    let text = std::fs::read_to_string(path)?;
    Ok(parse_os_release(&text))
}

pub fn parse_os_release(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), unquote(v.trim())))
        .collect()
}

fn unquote(v: &str) -> String {
    let inner = if v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')))
    {
        &v[1..v.len() - 1]
    } else {
        v
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn first_of(map: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|k| map.get(*k))
        .find(|v| !v.is_empty())
        .cloned()
}

/// Map an image build identifier to the server's numeric `build_id`.
pub fn build_id_from_str(s: &str) -> u64 {
    let s = s.trim();
    if s.is_empty() {
        return 0;
    }
    if s.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(n) = s.parse::<u64>() {
            return n;
        }
    }
    let digest = Sha256::digest(s.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(bytes)
}

pub fn device_id_from_machine_id(machine_id: &str) -> Result<String> {
    if machine_id.is_empty() {
        bail!("machine-id is empty (or set device_id)");
    }
    let digest = Sha256::digest(format!("ferrite-agent:{machine_id}").as_bytes());
    Ok(format!("linux-{}", &hex::encode(digest)[..16]))
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const YOCTO: &str = r#"
ID=poky
NAME="Poky (Yocto Project Reference Distro)"
VERSION="5.0.4 (scarthgap)"
VERSION_ID=5.0.4
VERSION_CODENAME="scarthgap"
PRETTY_NAME="Poky (Yocto Project Reference Distro) 5.0.4 (scarthgap)"
BUILD_ID="20261018093000"
IMAGE_VERSION="2.3.1"
"#;

    #[test]
    fn parses_quoted_and_escaped_values() {
        let m = parse_os_release(YOCTO);
        assert_eq!(m["NAME"], "Poky (Yocto Project Reference Distro)");
        assert_eq!(m["VERSION_ID"], "5.0.4");
        let m = parse_os_release("A='single'\nB=\"with \\\"quote\\\"\"\n# c=d\n");
        assert_eq!(m["A"], "single");
        assert_eq!(m["B"], "with \"quote\"");
        assert!(!m.contains_key("# c"));
    }

    #[test]
    fn build_id_mapping_is_stable() {
        assert_eq!(build_id_from_str("20261018093000"), 20261018093000);
        assert_eq!(build_id_from_str(" 42 "), 42);
        assert_eq!(build_id_from_str(""), 0);
        // Pinned (python: int.from_bytes(sha256(b"2.3.1").digest()[:8], "little")):
        // the mapping is a contract with whoever creates campaigns.
        assert_eq!(build_id_from_str("2.3.1"), 0x8609_78d9_bbea_c940);
        // Too large for u64: hashed rather than wrapped.
        assert_ne!(build_id_from_str("99999999999999999999"), 0);
        assert_eq!(
            build_id_from_str("99999999999999999999"),
            build_id_from_str("99999999999999999999")
        );
    }

    #[test]
    fn device_id_is_salted_hash() {
        let id = device_id_from_machine_id("0123456789abcdef0123456789abcdef").unwrap();
        assert!(id.starts_with("linux-"));
        assert_eq!(id.len(), 6 + 16);
        assert!(!id.contains("0123456789abcdef"));
        assert!(device_id_from_machine_id("").is_err());
    }

    fn cfg_with_root(root: &Path, extra: &str) -> Config {
        let mut cfg = Config::parse(&format!("server_url = \"http://x\"\n{extra}")).unwrap();
        cfg.root = root.to_path_buf();
        cfg
    }

    #[test]
    fn resolves_from_files_and_overrides() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("etc")).unwrap();
        std::fs::write(dir.path().join("etc/os-release"), YOCTO).unwrap();
        std::fs::write(dir.path().join("etc/machine-id"), "abc\n").unwrap();

        let id = Identity::resolve(&cfg_with_root(dir.path(), "")).unwrap();
        assert_eq!(id.firmware_version, "2.3.1");
        assert_eq!(id.build_id, 20261018093000);
        assert_eq!(id.build_id_source, "20261018093000");
        assert_eq!(id.device_id, device_id_from_machine_id("abc").unwrap());

        let id = Identity::resolve(&cfg_with_root(
            dir.path(),
            "device_id = \"plc-7\"\nfirmware_version = \"x\"\nbuild_id = 9",
        ))
        .unwrap();
        assert_eq!(
            (
                id.device_id.as_str(),
                id.firmware_version.as_str(),
                id.build_id
            ),
            ("plc-7", "x", 9)
        );
    }

    #[test]
    fn long_id_and_version_still_fit_one_chunk() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = cfg_with_root(
            dir.path(),
            &format!(
                "device_id = \"{}\"\nfirmware_version = \"{}\"",
                "d".repeat(127),
                "v".repeat(127)
            ),
        );
        let id = Identity::resolve(&cfg).unwrap();
        assert_eq!(id.device_id.len() + id.firmware_version.len(), 236);
    }

    #[test]
    fn falls_back_without_build_id() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("etc")).unwrap();
        std::fs::write(dir.path().join("etc/os-release"), "VERSION_ID=12\n").unwrap();
        let id = Identity::resolve(&cfg_with_root(dir.path(), "device_id = \"d\"")).unwrap();
        assert_eq!(id.firmware_version, "12");
        assert_eq!(id.build_id, 12);
    }
}
