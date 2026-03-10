use crate::api::types::*;
use crate::components::FaultViewer;
use dioxus::prelude::*;

#[component]
pub fn FaultsPage() -> Element {
    let mut severity_filter = use_signal(|| "all".to_string());
    let mut resolved_filter = use_signal(|| "all".to_string());

    let faults = vec![
        FaultEvent {
            id: "f-001".into(),
            device_id: "dev-001".into(),
            device_name: "Temperature Sensor A".into(),
            severity: FaultSeverity::Critical,
            code: "SENSOR_FAIL".into(),
            message: "ADC read failure on channel 0 - sensor may be disconnected".into(),
            timestamp: chrono::Utc::now(),
            resolved: false,
            resolved_at: None,
        },
        FaultEvent {
            id: "f-002".into(),
            device_id: "dev-002".into(),
            device_name: "Motor Controller B".into(),
            severity: FaultSeverity::Warning,
            code: "HIGH_VIBRATION".into(),
            message: "Vibration level exceeded 2.5g on axis Z".into(),
            timestamp: chrono::Utc::now(),
            resolved: false,
            resolved_at: None,
        },
        FaultEvent {
            id: "f-003".into(),
            device_id: "dev-003".into(),
            device_name: "Gateway Hub C".into(),
            severity: FaultSeverity::Info,
            code: "FW_AVAILABLE".into(),
            message: "New firmware version 3.1.0 available for download".into(),
            timestamp: chrono::Utc::now(),
            resolved: false,
            resolved_at: None,
        },
        FaultEvent {
            id: "f-004".into(),
            device_id: "dev-001".into(),
            device_name: "Temperature Sensor A".into(),
            severity: FaultSeverity::Warning,
            code: "TEMP_HIGH".into(),
            message: "Temperature exceeded threshold of 30C, reading 32.4C".into(),
            timestamp: chrono::Utc::now(),
            resolved: true,
            resolved_at: Some(chrono::Utc::now()),
        },
        FaultEvent {
            id: "f-005".into(),
            device_id: "dev-004".into(),
            device_name: "Pressure Sensor D".into(),
            severity: FaultSeverity::Critical,
            code: "CONN_LOST".into(),
            message: "Device connection lost, no heartbeat for 300s".into(),
            timestamp: chrono::Utc::now(),
            resolved: false,
            resolved_at: None,
        },
    ];

    let filtered: Vec<&FaultEvent> = faults
        .iter()
        .filter(|f| {
            let severity_ok = match severity_filter().as_str() {
                "critical" => matches!(f.severity, FaultSeverity::Critical),
                "warning" => matches!(f.severity, FaultSeverity::Warning),
                "info" => matches!(f.severity, FaultSeverity::Info),
                _ => true,
            };
            let resolved_ok = match resolved_filter().as_str() {
                "resolved" => f.resolved,
                "unresolved" => !f.resolved,
                _ => true,
            };
            severity_ok && resolved_ok
        })
        .collect();

    let critical_count = faults
        .iter()
        .filter(|f| matches!(f.severity, FaultSeverity::Critical) && !f.resolved)
        .count();
    let warning_count = faults
        .iter()
        .filter(|f| matches!(f.severity, FaultSeverity::Warning) && !f.resolved)
        .count();

    rsx! {
        div {
            class: "p-6 lg:p-8 max-w-[1400px] mx-auto",
            div {
                class: "mb-6 animate-fade-in",
                h1 {
                    class: "text-2xl font-semibold text-gray-100",
                    "Faults"
                }
                p {
                    class: "mt-1 text-sm text-gray-500",
                    "Device fault events and diagnostics"
                }
            }

            // Summary badges
            div {
                class: "flex items-center space-x-3 mb-6",
                if critical_count > 0 {
                    span {
                        class: "inline-flex items-center px-3 py-1 rounded-lg text-xs font-mono font-medium bg-red-500/10 text-red-400 border border-red-500/20",
                        "{critical_count} Critical"
                    }
                }
                if warning_count > 0 {
                    span {
                        class: "inline-flex items-center px-3 py-1 rounded-lg text-xs font-mono font-medium bg-amber-500/10 text-amber-400 border border-amber-500/20",
                        "{warning_count} Warning"
                    }
                }
            }

            // Filters
            div {
                class: "flex flex-col sm:flex-row gap-3 mb-6",
                select {
                    class: "px-4 py-2.5 bg-surface-900 border border-surface-700 rounded-lg text-sm text-gray-300 focus:ring-2 focus:ring-ferrite-500/40 focus:border-ferrite-600 outline-none transition-all",
                    value: "{severity_filter}",
                    onchange: move |e| severity_filter.set(e.value()),
                    option { value: "all", "All severities" }
                    option { value: "critical", "Critical" }
                    option { value: "warning", "Warning" }
                    option { value: "info", "Info" }
                }
                select {
                    class: "px-4 py-2.5 bg-surface-900 border border-surface-700 rounded-lg text-sm text-gray-300 focus:ring-2 focus:ring-ferrite-500/40 focus:border-ferrite-600 outline-none transition-all",
                    value: "{resolved_filter}",
                    onchange: move |e| resolved_filter.set(e.value()),
                    option { value: "all", "All states" }
                    option { value: "unresolved", "Unresolved" }
                    option { value: "resolved", "Resolved" }
                }
            }

            p {
                class: "text-[10px] text-gray-600 mb-4 font-mono uppercase tracking-wider",
                "{filtered.len()} fault(s)"
            }

            div {
                class: "space-y-3",
                for fault in filtered {
                    FaultViewer { fault: fault.clone() }
                }
            }
        }
    }
}
