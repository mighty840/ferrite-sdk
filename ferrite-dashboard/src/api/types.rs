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
