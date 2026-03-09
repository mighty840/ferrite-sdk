use crate::api::types::*;
use crate::components::{FaultViewer, MetricChart, TraceViewer};
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn DeviceDetailPage(id: String) -> Element {
    let mut active_tab = use_signal(|| "metrics".to_string());

    // Demo device data
    let device = Device {
        id: id.clone(),
        name: format!("Device {}", id),
        device_type: "sensor".into(),
        status: DeviceStatus::Online,
        firmware_version: "1.4.2".into(),
        last_seen: chrono::Utc::now(),
        ip_address: Some("192.168.1.10".into()),
        tags: vec!["production".into()],
    };

    let metrics: Vec<MetricEntry> = (0..20)
        .map(|i| MetricEntry {
            device_id: id.clone(),
            metric_name: "temperature".into(),
            value: 22.0 + (i as f64 * 0.3).sin() * 5.0,
            unit: "C".into(),
            timestamp: chrono::Utc::now(),
        })
        .collect();

    let humidity_metrics: Vec<MetricEntry> = (0..20)
        .map(|i| MetricEntry {
            device_id: id.clone(),
            metric_name: "humidity".into(),
            value: 55.0 + (i as f64 * 0.5).cos() * 15.0,
            unit: "%".into(),
            timestamp: chrono::Utc::now(),
        })
        .collect();

    let faults = vec![
        FaultEvent {
            id: "f-001".into(),
            device_id: id.clone(),
            device_name: device.name.clone(),
            severity: FaultSeverity::Warning,
            code: "TEMP_HIGH".into(),
            message: "Temperature exceeded threshold of 30C".into(),
            timestamp: chrono::Utc::now(),
            resolved: true,
            resolved_at: Some(chrono::Utc::now()),
        },
        FaultEvent {
            id: "f-002".into(),
            device_id: id.clone(),
            device_name: device.name.clone(),
            severity: FaultSeverity::Info,
            code: "FW_UPDATE".into(),
            message: "Firmware update available: v1.5.0".into(),
            timestamp: chrono::Utc::now(),
            resolved: false,
            resolved_at: None,
        },
    ];

    let traces = vec![
        TraceEntry {
            id: "t-001".into(),
            device_id: id.clone(),
            level: "INFO".into(),
            module: "sensor::adc".into(),
            message: "ADC calibration complete".into(),
            timestamp: chrono::Utc::now(),
            span_id: Some("span-abc".into()),
        },
        TraceEntry {
            id: "t-002".into(),
            device_id: id.clone(),
            level: "DEBUG".into(),
            module: "net::mqtt".into(),
            message: "Publishing telemetry to topic devices/dev-001/telemetry".into(),
            timestamp: chrono::Utc::now(),
            span_id: None,
        },
        TraceEntry {
            id: "t-003".into(),
            device_id: id.clone(),
            level: "WARN".into(),
            module: "sensor::temp".into(),
            message: "Temperature reading above warning threshold: 29.8C".into(),
            timestamp: chrono::Utc::now(),
            span_id: Some("span-def".into()),
        },
        TraceEntry {
            id: "t-004".into(),
            device_id: id.clone(),
            level: "INFO".into(),
            module: "sys::watchdog".into(),
            message: "Watchdog fed, system healthy".into(),
            timestamp: chrono::Utc::now(),
            span_id: None,
        },
        TraceEntry {
            id: "t-005".into(),
            device_id: id.clone(),
            level: "ERROR".into(),
            module: "net::mqtt".into(),
            message: "Connection lost, attempting reconnect in 5s".into(),
            timestamp: chrono::Utc::now(),
            span_id: Some("span-ghi".into()),
        },
    ];

    let status_badge = match device.status {
        DeviceStatus::Online => "bg-green-100 text-green-800",
        DeviceStatus::Offline => "bg-red-100 text-red-800",
        DeviceStatus::Degraded => "bg-yellow-100 text-yellow-800",
        DeviceStatus::Unknown => "bg-gray-100 text-gray-800",
    };

    let last_seen = device.last_seen.format("%Y-%m-%d %H:%M:%S UTC").to_string();

    rsx! {
        div {
            class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8",
            // Breadcrumb
            nav {
                class: "flex mb-6 text-sm",
                Link {
                    to: Route::Devices {},
                    class: "text-ferrite-600 hover:text-ferrite-800",
                    "Devices"
                }
                span { class: "mx-2 text-gray-400", "/" }
                span { class: "text-gray-700", "{device.name}" }
            }

            // Device header
            div {
                class: "bg-white rounded-lg shadow border border-gray-200 p-6 mb-6",
                div {
                    class: "flex flex-col md:flex-row md:items-center md:justify-between",
                    div {
                        class: "flex items-center space-x-4",
                        div {
                            class: "h-14 w-14 rounded-xl bg-ferrite-100 flex items-center justify-center",
                            svg {
                                class: "h-8 w-8 text-ferrite-600",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z"
                                }
                            }
                        }
                        div {
                            h1 {
                                class: "text-xl font-bold text-gray-900",
                                "{device.name}"
                            }
                            p {
                                class: "text-sm text-gray-500 font-mono",
                                "{device.id}"
                            }
                        }
                    }
                    div {
                        class: "mt-4 md:mt-0",
                        span {
                            class: "inline-flex items-center px-3 py-1 rounded-full text-sm font-medium {status_badge}",
                            "{device.status}"
                        }
                    }
                }
                div {
                    class: "mt-6 grid grid-cols-2 md:grid-cols-4 gap-4",
                    DetailField { label: "Type", value: device.device_type.clone() }
                    DetailField { label: "Firmware", value: device.firmware_version.clone() }
                    DetailField { label: "Last Seen", value: last_seen }
                    DetailField {
                        label: "IP Address",
                        value: device.ip_address.clone().unwrap_or_else(|| "N/A".into()),
                    }
                }
            }

            // Tabs
            div {
                class: "border-b border-gray-200 mb-6",
                nav {
                    class: "flex space-x-8",
                    TabButton {
                        label: "Metrics",
                        tab_id: "metrics",
                        active_tab: active_tab(),
                        on_click: move |_| active_tab.set("metrics".into()),
                    }
                    TabButton {
                        label: "Faults",
                        tab_id: "faults",
                        active_tab: active_tab(),
                        on_click: move |_| active_tab.set("faults".into()),
                    }
                    TabButton {
                        label: "Traces",
                        tab_id: "traces",
                        active_tab: active_tab(),
                        on_click: move |_| active_tab.set("traces".into()),
                    }
                }
            }

            // Tab content
            match active_tab().as_str() {
                "metrics" => rsx! {
                    div {
                        class: "grid grid-cols-1 lg:grid-cols-2 gap-6",
                        MetricChart {
                            title: "Temperature".to_string(),
                            entries: metrics.clone(),
                            color: Some("#ef4444".to_string()),
                        }
                        MetricChart {
                            title: "Humidity".to_string(),
                            entries: humidity_metrics.clone(),
                            color: Some("#3b82f6".to_string()),
                        }
                    }
                },
                "faults" => rsx! {
                    div {
                        class: "space-y-4",
                        for fault in &faults {
                            FaultViewer { fault: fault.clone() }
                        }
                    }
                },
                "traces" => rsx! {
                    TraceViewer { traces: traces.clone() }
                },
                _ => rsx! {
                    p { "Unknown tab" }
                },
            }
        }
    }
}

#[component]
fn DetailField(label: String, value: String) -> Element {
    rsx! {
        div {
            p {
                class: "text-xs font-medium text-gray-500 uppercase tracking-wide",
                "{label}"
            }
            p {
                class: "mt-1 text-sm text-gray-900",
                "{value}"
            }
        }
    }
}

#[component]
fn TabButton(
    label: &'static str,
    tab_id: &'static str,
    active_tab: String,
    on_click: EventHandler<MouseEvent>,
) -> Element {
    let is_active = active_tab == tab_id;
    let classes = if is_active {
        "border-ferrite-500 text-ferrite-600 border-b-2 py-3 px-1 text-sm font-medium cursor-pointer"
    } else {
        "border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300 border-b-2 py-3 px-1 text-sm font-medium cursor-pointer"
    };

    rsx! {
        button {
            class: classes,
            onclick: move |e| on_click.call(e),
            "{label}"
        }
    }
}
