use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Device {
    pub id: i64,
    pub device_id: String,
    pub firmware_version: String,
    pub build_id: u64,
    pub first_seen: String,
    pub last_seen: String,
    pub device_key: Option<i64>,
    pub name: Option<String>,
    pub status: Option<String>,
    pub tags: Option<String>,
    pub provisioned_by: Option<String>,
    pub provisioned_at: Option<String>,
}

impl Device {
    /// Format device_key as "XX-YYYYYY" hex, or "N/A" if not set.
    pub fn key_display(&self) -> String {
        match self.device_key {
            Some(k) => {
                let k = k as u32;
                let prefix = (k >> 24) as u8;
                let suffix = k & 0x00FF_FFFF;
                format!("{:02X}-{:06X}", prefix, suffix)
            }
            None => "N/A".to_string(),
        }
    }

    /// Display name or device_id as fallback.
    pub fn display_name(&self) -> String {
        self.name
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| self.device_id.clone())
    }

    /// Status string, defaulting to "unknown".
    pub fn status_str(&self) -> &str {
        self.status.as_deref().unwrap_or("unknown")
    }

    /// Parse tags JSON string into a list.
    pub fn tags_list(&self) -> Vec<String> {
        self.tags
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| {
                // Try JSON array first, then comma-separated
                serde_json::from_str::<Vec<String>>(s).unwrap_or_else(|_| {
                    s.split(',')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect()
                })
            })
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaultEvent {
    pub id: i64,
    pub device_rowid: i64,
    pub device_id: String,
    pub fault_type: u8,
    pub pc: u32,
    pub lr: u32,
    pub cfsr: u32,
    pub hfsr: u32,
    pub mmfar: u32,
    pub bfar: u32,
    pub sp: u32,
    pub stack_snapshot: String,
    pub symbol: Option<String>,
    pub created_at: String,
}

impl FaultEvent {
    pub fn fault_type_name(&self) -> &'static str {
        match self.fault_type {
            0 => "HardFault",
            1 => "MemManage",
            2 => "BusFault",
            3 => "UsageFault",
            _ => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricRow {
    pub id: i64,
    pub device_rowid: i64,
    pub device_id: String,
    pub key: String,
    pub metric_type: u8,
    pub value_json: String,
    pub timestamp_ticks: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CrashGroup {
    pub id: i64,
    pub signature_hash: String,
    pub pc: u32,
    pub fault_type: u8,
    pub first_seen: String,
    pub last_seen: String,
    pub occurrence_count: i64,
    pub affected_device_count: i64,
    pub title: Option<String>,
}

impl CrashGroup {
    pub fn fault_type_name(&self) -> &'static str {
        match self.fault_type {
            0 => "HardFault",
            1 => "MemManage",
            2 => "BusFault",
            3 => "UsageFault",
            _ => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OtaCampaign {
    pub id: i64,
    pub name: String,
    pub firmware_id: i64,
    pub target_version: String,
    pub strategy: String,
    pub target_group_id: Option<i64>,
    pub target_tags: Option<String>,
    pub rollout_percent: i64,
    pub failure_threshold: f64,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl OtaCampaign {
    pub fn status_color(&self) -> &'static str {
        match self.status.as_str() {
            "active" => "text-green-400 bg-green-400/10 border-green-500/20",
            "paused" => "text-yellow-400 bg-yellow-400/10 border-yellow-500/20",
            "completed" => "text-blue-400 bg-blue-400/10 border-blue-500/20",
            "rolled_back" => "text-orange-400 bg-orange-400/10 border-orange-500/20",
            "failed" => "text-red-400 bg-red-400/10 border-red-500/20",
            _ => "text-gray-400 bg-gray-400/10 border-gray-500/20",
        }
    }

    pub fn strategy_color(&self) -> &'static str {
        match self.strategy.as_str() {
            "canary" => "text-amber-400 bg-amber-400/10 border-amber-500/20",
            "scheduled" => "text-blue-400 bg-blue-400/10 border-blue-500/20",
            _ => "text-gray-400 bg-gray-400/10 border-gray-500/20",
        }
    }

    pub fn can_activate(&self) -> bool {
        matches!(self.status.as_str(), "created" | "paused")
    }

    pub fn can_pause(&self) -> bool {
        self.status == "active"
    }

    pub fn can_rollback(&self) -> bool {
        matches!(self.status.as_str(), "active" | "paused")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CampaignSummary {
    pub campaign: OtaCampaign,
    pub pending: i64,
    pub downloading: i64,
    pub installed: i64,
    pub failed: i64,
}

impl CampaignSummary {
    pub fn total_devices(&self) -> i64 {
        self.pending + self.downloading + self.installed + self.failed
    }

    pub fn progress_pct(&self) -> u32 {
        let total = self.total_devices();
        if total == 0 {
            return 0;
        }
        ((self.installed * 100) / total) as u32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CampaignDevice {
    pub id: i64,
    pub campaign_id: i64,
    pub device_id: String,
    pub status: String,
    pub updated_at: String,
}

impl CampaignDevice {
    pub fn status_color(&self) -> &'static str {
        match self.status.as_str() {
            "installed" => "text-green-400 bg-green-400/10 border-green-500/20",
            "downloading" => "text-blue-400 bg-blue-400/10 border-blue-500/20",
            "failed" => "text-red-400 bg-red-400/10 border-red-500/20",
            _ => "text-gray-400 bg-gray-400/10 border-gray-500/20",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FirmwareArtifact {
    pub id: i64,
    pub version: String,
    pub build_id: i64,
    pub sha256: String,
    pub size: i64,
    pub filename: String,
    pub signer: Option<String>,
    pub created_at: String,
}

impl FirmwareArtifact {
    pub fn size_display(&self) -> String {
        if self.size >= 1024 * 1024 {
            format!("{:.1} MB", self.size as f64 / (1024.0 * 1024.0))
        } else {
            format!("{} KB", self.size / 1024)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TraceEntry {
    pub id: String,
    pub device_id: String,
    pub level: String,
    pub module: String,
    pub message: String,
    pub timestamp: String,
    pub span_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveEvent {
    pub event_type: String,
    pub device_id: String,
    pub payload: serde_json::Value,
    pub timestamp: String,
}

#[derive(Debug, Clone)]
pub enum ApiError {
    Network(String),
    Unauthorized,
    NotFound,
    Server(String),
    Parse(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Network(msg) => write!(f, "Network error: {}", msg),
            ApiError::Unauthorized => write!(f, "Unauthorized - please log in"),
            ApiError::NotFound => write!(f, "Resource not found"),
            ApiError::Server(msg) => write!(f, "Server error: {}", msg),
            ApiError::Parse(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}
