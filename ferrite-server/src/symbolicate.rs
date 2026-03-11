use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// ELF symbolication via `arm-none-eabi-addr2line`.
pub struct Symbolicator {
    /// Path to the addr2line binary.
    addr2line_path: PathBuf,
    /// Registered ELF files keyed by firmware version.
    elfs_by_version: HashMap<String, PathBuf>,
    /// The ELF directory, used to discover pre-existing files.
    elf_dir: PathBuf,
    /// The "current" ELF path (most recently registered or the only one available).
    current_elf: Option<PathBuf>,
}

impl Symbolicator {
    pub fn new(addr2line_override: Option<PathBuf>, elf_dir: PathBuf) -> Self {
        let addr2line_path = addr2line_override.unwrap_or_else(|| {
            // Try to auto-detect
            which_addr2line().unwrap_or_else(|| PathBuf::from("arm-none-eabi-addr2line"))
        });

        let mut s = Self {
            addr2line_path,
            elfs_by_version: HashMap::new(),
            elf_dir,
            current_elf: None,
        };
        s.discover_existing_elfs();
        s
    }

    /// Scan the elf directory for any pre-existing .elf files and register them.
    fn discover_existing_elfs(&mut self) {
        if let Ok(entries) = std::fs::read_dir(&self.elf_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("elf") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        self.elfs_by_version.insert(stem.to_string(), path.clone());
                        self.current_elf = Some(path);
                    }
                }
            }
        }
    }

    /// Register an ELF file for a given firmware version.
    pub fn register_elf(&mut self, version: &str, path: PathBuf) {
        self.current_elf = Some(path.clone());
        self.elfs_by_version.insert(version.to_string(), path);
    }

    /// Get the ELF path for a given firmware version, falling back to
    /// the current ELF if the version is not found.
    fn elf_for_version(&self, version: Option<&str>) -> Option<&PathBuf> {
        if let Some(v) = version {
            if let Some(p) = self.elfs_by_version.get(v) {
                return Some(p);
            }
        }
        self.current_elf.as_ref()
    }

    /// Symbolize a single program counter address.
    /// Returns the symbolicated string (e.g., "main at src/main.c:42") or None.
    pub async fn symbolize(&self, pc: u32) -> Result<Option<String>, SymbolicateError> {
        self.symbolize_with_version(pc, None).await
    }

    /// Symbolize a PC address using a specific firmware version's ELF.
    pub async fn symbolize_with_version(
        &self,
        pc: u32,
        version: Option<&str>,
    ) -> Result<Option<String>, SymbolicateError> {
        let elf_path = match self.elf_for_version(version) {
            Some(p) => p.clone(),
            None => return Ok(None),
        };

        let addr = format!("0x{pc:08X}");
        let addr2line = self.addr2line_path.clone();

        // Run addr2line in a blocking task to avoid blocking the async runtime.
        let result =
            tokio::task::spawn_blocking(move || run_addr2line(&addr2line, &elf_path, &addr))
                .await
                .map_err(|e| SymbolicateError::JoinError(e.to_string()))?;

        result
    }

    /// Symbolize a fault: resolves both PC and LR.
    pub async fn symbolize_fault(
        &self,
        pc: u32,
        lr: u32,
        version: Option<&str>,
    ) -> Result<FaultSymbols, SymbolicateError> {
        let pc_sym = self.symbolize_with_version(pc, version).await?;
        let lr_sym = self.symbolize_with_version(lr, version).await?;
        Ok(FaultSymbols {
            pc_symbol: pc_sym,
            lr_symbol: lr_sym,
        })
    }
}

/// Result of symbolizing a fault's PC and LR.
#[derive(Debug, Clone)]
pub struct FaultSymbols {
    pub pc_symbol: Option<String>,
    pub lr_symbol: Option<String>,
}

#[derive(Debug)]
pub enum SymbolicateError {
    IoError(std::io::Error),
    JoinError(String),
    Timeout,
}

impl std::fmt::Display for SymbolicateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "I/O error: {e}"),
            Self::JoinError(e) => write!(f, "join error: {e}"),
            Self::Timeout => write!(f, "addr2line timed out after 10s"),
        }
    }
}

impl std::error::Error for SymbolicateError {}

/// Try to find `arm-none-eabi-addr2line` on PATH.
fn which_addr2line() -> Option<PathBuf> {
    let candidates = ["arm-none-eabi-addr2line", "arm-none-eabi-addr2line.exe"];
    for candidate in &candidates {
        if let Ok(output) = Command::new("which").arg(candidate).output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(PathBuf::from(path));
                }
            }
        }
    }
    None
}

/// Shell out to addr2line and parse the result.
/// Kills the subprocess if it does not complete within 10 seconds.
fn run_addr2line(
    addr2line: &std::path::Path,
    elf_path: &std::path::Path,
    addr: &str,
) -> Result<Option<String>, SymbolicateError> {
    let mut child = Command::new(addr2line)
        .args(["-e", &elf_path.to_string_lossy(), "-f", "-C", "-p", addr])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(SymbolicateError::IoError)?;

    let timeout = std::time::Duration::from_secs(10);
    let start = std::time::Instant::now();
    loop {
        match child.try_wait().map_err(SymbolicateError::IoError)? {
            Some(_status) => {
                let output = child.wait_with_output().map_err(SymbolicateError::IoError)?;

                if !output.status.success() {
                    return Ok(None);
                }

                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

                // addr2line returns "?? ??:0" or similar when it can't resolve.
                if stdout.is_empty() || stdout.starts_with("?? ") || stdout.contains("??:0") {
                    return Ok(None);
                }

                return Ok(Some(stdout));
            }
            None if start.elapsed() > timeout => {
                let _ = child.kill();
                return Err(SymbolicateError::Timeout);
            }
            None => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
}
