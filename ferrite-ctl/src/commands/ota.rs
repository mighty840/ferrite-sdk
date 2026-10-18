use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use ed25519_compact::{KeyPair, Seed};
use ferrite_sdk::ota_manifest::{key_id, signing_message, Manifest, SignedManifest, VERSION_LEN};
use sha2::{Digest, Sha256};

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
    /// Generate an Ed25519 signing key pair (offline)
    Keygen {
        /// Secret key file to create; the public key goes to `<out>.pub`
        #[arg(long)]
        out: PathBuf,
    },
    /// Sign a firmware image, writing its manifest (offline)
    Sign {
        /// Secret key file from `ferrite ota keygen`
        #[arg(long)]
        key: PathBuf,
        /// Raw firmware image as the bootloader expects it (.bin, not ELF)
        #[arg(long)]
        firmware: PathBuf,
        /// Firmware version string (max 32 bytes)
        #[arg(long)]
        version: String,
        /// Build ID the firmware reports (decimal or 0x-prefixed hex)
        #[arg(long, value_parser = parse_u64)]
        build_id: u64,
        /// Anti-rollback counter; devices reject manifests below their minimum
        #[arg(long, default_value = "0")]
        security_version: u32,
        /// Manifest output path (default: `<firmware>.manifest`)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Check a manifest and image against a public key (offline)
    Verify {
        /// Manifest file from `ferrite ota sign`
        #[arg(long)]
        manifest: PathBuf,
        /// Firmware image the manifest describes
        #[arg(long)]
        firmware: PathBuf,
        /// Public key: a `.pub` file or 64 hex chars
        #[arg(long)]
        pubkey: String,
    },
    /// Upload a firmware image (with its manifest) to the server
    Upload {
        /// Firmware image
        #[arg(long)]
        firmware: PathBuf,
        /// Manifest file (default: `<firmware>.manifest` if it exists)
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Version, only for unsigned uploads
        #[arg(long)]
        version: Option<String>,
        /// Build ID, only for unsigned uploads
        #[arg(long, value_parser = parse_u64)]
        build_id: Option<u64>,
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
        OtaCommand::Keygen { out } => keygen(out)?,
        OtaCommand::Sign {
            key,
            firmware,
            version,
            build_id,
            security_version,
            out,
        } => {
            let seed = read_seed(key)?;
            let image = read_file(firmware)?;
            let signed = sign_image(&seed, &image, version, *build_id, *security_version)?;
            let out = out.clone().unwrap_or_else(|| manifest_path(firmware));
            fs::write(&out, signed.to_bytes())
                .with_context(|| format!("failed to write {}", out.display()))?;
            let m = signed.unverified();
            println!("Signed {} ({} bytes)", firmware.display(), m.image_size);
            println!("  version          {}", m.version);
            println!("  build_id         {}", m.build_id);
            println!("  security_version {}", m.security_version);
            println!("  key_id           {}", hex::encode(m.key_id));
            println!("Manifest: {}", out.display());
        }
        OtaCommand::Verify {
            manifest,
            firmware,
            pubkey,
        } => {
            let public_key = read_public_key(pubkey)?;
            let signed = SignedManifest::from_bytes(&read_file(manifest)?)
                .map_err(|e| anyhow::anyhow!("invalid manifest: {e:?}"))?;
            verify_image(&signed, &read_file(firmware)?, &public_key)?;
            let m = signed.unverified();
            println!(
                "OK: {} build {} (security_version {}) signed by {}",
                m.version,
                m.build_id,
                m.security_version,
                hex::encode(m.key_id)
            );
        }
        OtaCommand::Upload {
            firmware,
            manifest,
            version,
            build_id,
        } => {
            let image = read_file(firmware)?;
            let manifest = match manifest {
                Some(p) => Some(read_file(p)?),
                None => {
                    let default = manifest_path(firmware);
                    if default.exists() {
                        Some(read_file(&default)?)
                    } else {
                        None
                    }
                }
            };
            let manifest_hex = match &manifest {
                Some(bytes) => {
                    let signed = SignedManifest::from_bytes(bytes)
                        .map_err(|e| anyhow::anyhow!("invalid manifest: {e:?}"))?;
                    Some(String::from_utf8_lossy(&signed.to_hex()).into_owned())
                }
                None => {
                    eprintln!("warning: uploading without a manifest; devices will refuse it");
                    None
                }
            };
            let artifact = client.upload_firmware(
                image,
                manifest_hex.as_deref(),
                version.as_deref(),
                *build_id,
            )?;
            println!(
                "Uploaded firmware #{}: {} build {} (signer: {})",
                artifact["id"],
                artifact["version"].as_str().unwrap_or("?"),
                artifact["build_id"],
                artifact["signer"].as_str().unwrap_or("none"),
            );
        }
    }
    Ok(())
}

fn keygen(out: &Path) -> Result<()> {
    let pub_path = out.with_extension(match out.extension() {
        Some(ext) => format!("{}.pub", ext.to_string_lossy()),
        None => "pub".into(),
    });
    let kp = KeyPair::generate();
    let seed = kp.sk.seed();

    write_new_file(out, format!("{}\n", hex::encode(*seed)).as_bytes(), true)?;
    write_new_file(
        &pub_path,
        format!("{}\n", hex::encode(*kp.pk)).as_bytes(),
        false,
    )?;

    let pk: [u8; 32] = *kp.pk;
    println!("Secret key: {} (keep offline, never commit)", out.display());
    println!("Public key: {}", pub_path.display());
    println!("Key ID:     {}", hex::encode(key_id(&pk)));
    println!();
    println!("Server:   OTA_TRUSTED_KEYS={}", hex::encode(pk));
    println!("Firmware: {}", rust_key_const(&pk));
    Ok(())
}

/// Build and sign a manifest for `image`.
pub fn sign_image(
    seed: &[u8; 32],
    image: &[u8],
    version: &str,
    build_id: u64,
    security_version: u32,
) -> Result<SignedManifest> {
    if version.len() > VERSION_LEN {
        bail!("version must be at most {VERSION_LEN} bytes");
    }
    if version.contains('\0') {
        bail!("version must not contain NUL");
    }
    let image_size = u32::try_from(image.len()).context("firmware image larger than 4 GiB")?;

    let kp = KeyPair::from_seed(Seed::new(*seed));
    let pk: [u8; 32] = *kp.pk;
    let mut v = heapless::String::new();
    v.push_str(version)
        .map_err(|_| anyhow::anyhow!("version too long"))?;
    let manifest = Manifest {
        build_id,
        security_version,
        image_size,
        sha256: Sha256::digest(image).into(),
        version: v,
        key_id: key_id(&pk),
    };
    let body = manifest.encode_body();
    let signature = kp.sk.sign(signing_message(&body), None);
    SignedManifest::new(body, *signature).map_err(|e| anyhow::anyhow!("manifest: {e:?}"))
}

/// Check signature, size and hash of `image` against `signed`.
pub fn verify_image(signed: &SignedManifest, image: &[u8], public_key: &[u8; 32]) -> Result<()> {
    let manifest = signed
        .verify(&[*public_key])
        .map_err(|e| anyhow::anyhow!("signature check failed: {e:?}"))?;
    let mut verifier = ferrite_sdk::ota_manifest::ImageVerifier::new(manifest);
    verifier
        .update(image)
        .and_then(|_| verifier.finish())
        .map_err(|e| anyhow::anyhow!("image does not match manifest: {e:?}"))
}

fn manifest_path(firmware: &Path) -> PathBuf {
    let mut p = firmware.as_os_str().to_owned();
    p.push(".manifest");
    PathBuf::from(p)
}

fn read_file(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).with_context(|| format!("failed to read {}", path.display()))
}

fn read_seed(path: &Path) -> Result<[u8; 32]> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read key {}", path.display()))?;
    parse_hex32(text.trim()).with_context(|| format!("{} is not a ferrite OTA key", path.display()))
}

fn read_public_key(arg: &str) -> Result<[u8; 32]> {
    if let Ok(key) = parse_hex32(arg.trim()) {
        return Ok(key);
    }
    let text =
        fs::read_to_string(arg).with_context(|| format!("failed to read public key {arg}"))?;
    parse_hex32(text.trim()).with_context(|| format!("{arg} is not a public key"))
}

fn parse_hex32(s: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(s).context("not valid hex")?;
    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| anyhow::anyhow!("expected 32 bytes"))
}

fn parse_u64(s: &str) -> Result<u64, String> {
    let parsed = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => s.parse(),
    };
    parsed.map_err(|e| format!("invalid number {s:?}: {e}"))
}

fn rust_key_const(pk: &[u8; 32]) -> String {
    let bytes: Vec<String> = pk.iter().map(|b| format!("0x{b:02x}")).collect();
    format!(
        "pub const OTA_TRUSTED_KEYS: [[u8; 32]; 1] = [[{}]];",
        bytes.join(", ")
    )
}

/// Create a file that must not exist yet; secret files are owner-only.
fn write_new_file(path: &Path, contents: &[u8], secret: bool) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if secret { 0o600 } else { 0o644 });
    }
    #[cfg(not(unix))]
    let _ = secret;
    let mut file = options.open(path).with_context(|| {
        format!(
            "failed to create {} (does it already exist?)",
            path.display()
        )
    })?;
    file.write_all(contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: [u8; 32] = [3; 32];

    fn public_key() -> [u8; 32] {
        *KeyPair::from_seed(Seed::new(SEED)).pk
    }

    #[test]
    fn sign_then_verify_roundtrip() {
        let image = vec![0xEE; 1000];
        let signed = sign_image(&SEED, &image, "1.0.0", 0xABCD, 4).unwrap();
        let parsed = SignedManifest::from_bytes(&signed.to_bytes()).unwrap();
        verify_image(&parsed, &image, &public_key()).unwrap();
        assert_eq!(parsed.unverified().security_version, 4);
    }

    #[test]
    fn verify_rejects_other_image_and_key() {
        let image = vec![0xEE; 1000];
        let signed = sign_image(&SEED, &image, "1.0.0", 1, 0).unwrap();
        assert!(verify_image(&signed, &[0xEF; 1000], &public_key()).is_err());
        assert!(verify_image(&signed, &image, &[9; 32]).is_err());
    }

    #[test]
    fn version_limits() {
        assert!(sign_image(&SEED, b"x", &"v".repeat(33), 1, 0).is_err());
        assert!(sign_image(&SEED, b"x", &"v".repeat(32), 1, 0).is_ok());
        assert!(sign_image(&SEED, b"x", "a\0b", 1, 0).is_err());
    }

    #[test]
    fn build_id_parsing() {
        assert_eq!(parse_u64("42"), Ok(42));
        assert_eq!(parse_u64("0xDEADBEEF"), Ok(0xDEAD_BEEF));
        assert!(parse_u64("nope").is_err());
    }

    #[test]
    fn keygen_writes_owner_only_secret_and_refuses_overwrite() {
        let dir = std::env::temp_dir().join(format!("ferrite-keygen-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let key = dir.join("ota.key");
        keygen(&key).unwrap();

        let seed = read_seed(&key).unwrap();
        let pk = read_public_key(dir.join("ota.key.pub").to_str().unwrap()).unwrap();
        assert_eq!(*KeyPair::from_seed(Seed::new(seed)).pk, pk);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&key).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(keygen(&key).is_err(), "must not overwrite an existing key");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn manifest_path_appends_suffix() {
        assert_eq!(
            manifest_path(Path::new("out/fw.bin")),
            PathBuf::from("out/fw.bin.manifest")
        );
    }
}
