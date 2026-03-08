#![no_std]

pub mod chunks;
pub mod config;
pub mod memory;
pub mod reboot_reason;
pub mod fault;
pub mod metrics;
pub mod trace;
pub mod transport;
pub mod upload;
pub mod sdk;

#[cfg(feature = "defmt")]
pub mod defmt_sink;

/// Error type for all public SDK APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdkError {
    NotInitialized,
    AlreadyInitialized,
    BufferFull,
    KeyTooLong,
    TooManyRamRegions,
    InvalidConfig,
    EncodingFailed,
}

impl core::fmt::Display for SdkError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "SDK not initialized"),
            Self::AlreadyInitialized => write!(f, "SDK already initialized"),
            Self::BufferFull => write!(f, "buffer full"),
            Self::KeyTooLong => write!(f, "metric key too long (max 32 chars)"),
            Self::TooManyRamRegions => write!(f, "too many RAM regions (max 4)"),
            Self::InvalidConfig => write!(f, "invalid SDK config"),
            Self::EncodingFailed => write!(f, "chunk encoding failed"),
        }
    }
}

// Re-export key types at crate root
pub use reboot_reason::RebootReason;
pub use fault::RamRegion;
pub use sdk::{SdkConfig, init, is_initialized, with_sdk};
pub use metrics::ticks;
