//! Build-time firmware identity and OTA trust anchor.
//!
//! - `FERRITE_BUILD_ID` (decimal or 0x-hex): the build id this firmware
//!   reports and that `ferrite ota sign --build-id` must use. Falls back to
//!   `SOURCE_DATE_EPOCH`, then the build time. Set it once in the release
//!   script and pass the same value to the signer.
//! - `FERRITE_FIRMWARE_VERSION`: version string the firmware reports and that
//!   `ferrite ota sign --version` must use (max 32 bytes, the manifest field
//!   size). Defaults to the crate version.
//! - `FERRITE_OTA_PUBKEY`: trusted Ed25519 public key(s) for OTA manifests,
//!   comma-separated hex or paths to `.pub` files from `ferrite ota keygen`.
//!   Required: firmware without a trust anchor could never verify an update.
//! - `FERRITE_SECURITY_VERSION`: anti-rollback floor (default 0).
//! - `FERRITE_TEST_HANG_AFTER_BOOT=1`: test builds only. The image hangs
//!   after link-up instead of confirming, so the watchdog resets it and the
//!   bootloader rolls back. Used to demonstrate rollback; never ship it.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Linker args here, not in .cargo/config.toml, so builds from the repo
    // root (`--manifest-path`, as CI does) link the same way.
    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-search={dir}");
    println!("cargo:rustc-link-arg=-Tlink.x");
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    println!("cargo:rerun-if-changed=memory.x");
    for var in [
        "FERRITE_BUILD_ID",
        "FERRITE_FIRMWARE_VERSION",
        "SOURCE_DATE_EPOCH",
        "FERRITE_OTA_PUBKEY",
        "FERRITE_SECURITY_VERSION",
        "FERRITE_TEST_HANG_AFTER_BOOT",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }

    let build_id = match env::var("FERRITE_BUILD_ID") {
        Ok(v) => parse_u64(&v).unwrap_or_else(|| panic!("FERRITE_BUILD_ID={v:?} is not a number")),
        Err(_) => env::var("SOURCE_DATE_EPOCH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }),
    };
    let firmware_version = env::var("FERRITE_FIRMWARE_VERSION")
        .unwrap_or_else(|_| env::var("CARGO_PKG_VERSION").unwrap());
    assert!(
        !firmware_version.is_empty()
            && firmware_version.len() <= 32
            && !firmware_version.contains('\0'),
        "FERRITE_FIRMWARE_VERSION must be 1-32 bytes without NUL (manifest field size)"
    );
    println!(
        "cargo:warning=ferrite firmware {firmware_version} build_id {build_id} \
         (sign with --version {firmware_version} --build-id {build_id})"
    );

    println!("cargo::rustc-check-cfg=cfg(test_hang_after_boot)");
    if env::var("FERRITE_TEST_HANG_AFTER_BOOT").as_deref() == Ok("1") {
        println!("cargo:rustc-cfg=test_hang_after_boot");
        println!("cargo:warning=TEST BUILD: hangs after boot to exercise rollback");
    }

    let keys_var = env::var("FERRITE_OTA_PUBKEY").unwrap_or_else(|_| {
        panic!(
            "set FERRITE_OTA_PUBKEY to the OTA public key (hex, or path to the .pub file \
             from `ferrite ota keygen`)"
        )
    });
    let keys: Vec<[u8; 32]> = keys_var
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            let text = if s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
                s.to_string()
            } else {
                println!("cargo:rerun-if-changed={s}");
                fs::read_to_string(s)
                    .unwrap_or_else(|e| panic!("FERRITE_OTA_PUBKEY: cannot read {s}: {e}"))
            };
            parse_key(text.trim()).unwrap_or_else(|| panic!("FERRITE_OTA_PUBKEY: bad key {s:?}"))
        })
        .collect();
    assert!(!keys.is_empty(), "FERRITE_OTA_PUBKEY is empty");

    let security_version: u32 = env::var("FERRITE_SECURITY_VERSION")
        .map(|v| v.parse().expect("FERRITE_SECURITY_VERSION must be a u32"))
        .unwrap_or(0);

    let key_list: Vec<String> = keys.iter().map(|k| format!("{k:?}")).collect();
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("firmware_identity.rs");
    fs::write(
        out,
        format!(
            "pub const BUILD_ID: u64 = {build_id};\n\
             pub const FIRMWARE_VERSION: &str = {firmware_version:?};\n\
             pub const SECURITY_VERSION: u32 = {security_version};\n\
             pub const OTA_TRUSTED_KEYS: [[u8; 32]; {}] = [{}];\n",
            keys.len(),
            key_list.join(", ")
        ),
    )
    .unwrap();
}

fn parse_u64(s: &str) -> Option<u64> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

fn parse_key(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(key)
}
