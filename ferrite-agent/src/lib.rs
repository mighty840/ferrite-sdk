//! ferrite-agent: reports an embedded Linux device to ferrite-server.
//!
//! The Linux counterpart of the MCU SDK. It speaks the same chunk format
//! over `POST /ingest/chunks`, so Linux devices appear in the device list,
//! metrics, OTA campaigns and update-reach reports like any MCU.

pub mod boot;
pub mod config;
pub mod identity;
pub mod report;
pub mod services;
pub mod system;
pub mod uplink;
