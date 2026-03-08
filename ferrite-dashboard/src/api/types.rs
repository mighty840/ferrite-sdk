use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceStatus {
    Online,
    Offline,
    Degraded,
    Unknown,
}

impl std::fmt::Display for DeviceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceStatus::Online => write!(f, "Online"),
            DeviceStatus::Offline => write!(f, "Offline"),
            DeviceStatus::Degraded => write!(f, "Degraded"),
            DeviceStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub status: DeviceStatus,
    pub firmware_version: String,
    pub last_seen: DateTime<Utc>,
    pub ip_address: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FaultSeverity {
    Critical,
    Warning,
    Info,
}

impl std::fmt::Display for FaultSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FaultSeverity::Critical => write!(f, "Critical"),
            FaultSeverity::Warning => write!(f, "Warning"),
            FaultSeverity::Info => write!(f, "Info"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaultEvent {
    pub id: String,
    pub device_id: String,
    pub device_name: String,
    pub severity: FaultSeverity,
    pub code: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub resolved: bool,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricEntry {
    pub device_id: String,
    pub metric_name: String,
    pub value: f64,
    pub unit: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveEvent {
    pub event_type: String,
    pub device_id: String,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TraceEntry {
    pub id: String,
    pub device_id: String,
    pub level: String,
    pub module: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub span_id: Option<String>,
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
