use crate::api::types::MetricEntry;
use crate::components::MetricChart;
use dioxus::prelude::*;

#[component]
pub fn MetricsPage() -> Element {
    // Demo metric data
    let temperature: Vec<MetricEntry> = (0..30)
        .map(|i| MetricEntry {
            device_id: "dev-001".into(),
            metric_name: "temperature".into(),
            value: 22.0 + (i as f64 * 0.3).sin() * 5.0 + (i as f64 * 0.1),
            unit: "C".into(),
            timestamp: chrono::Utc::now(),
        })
        .collect();

    let humidity: Vec<MetricEntry> = (0..30)
        .map(|i| MetricEntry {
            device_id: "dev-001".into(),
            metric_name: "humidity".into(),
            value: 55.0 + (i as f64 * 0.5).cos() * 15.0,
            unit: "%".into(),
            timestamp: chrono::Utc::now(),
        })
        .collect();

    let pressure: Vec<MetricEntry> = (0..30)
        .map(|i| MetricEntry {
            device_id: "dev-004".into(),
            metric_name: "pressure".into(),
            value: 1013.0 + (i as f64 * 0.2).sin() * 8.0,
            unit: "hPa".into(),
            timestamp: chrono::Utc::now(),
        })
        .collect();

    let vibration: Vec<MetricEntry> = (0..30)
        .map(|i| MetricEntry {
            device_id: "dev-002".into(),
            metric_name: "vibration".into(),
            value: 0.5 + (i as f64 * 0.8).sin().abs() * 2.0,
            unit: "g".into(),
            timestamp: chrono::Utc::now(),
        })
        .collect();

    let cpu_usage: Vec<MetricEntry> = (0..30)
        .map(|i| MetricEntry {
            device_id: "dev-003".into(),
            metric_name: "cpu_usage".into(),
            value: 35.0 + (i as f64 * 0.4).sin() * 20.0,
            unit: "%".into(),
            timestamp: chrono::Utc::now(),
        })
        .collect();

    let memory: Vec<MetricEntry> = (0..30)
        .map(|i| MetricEntry {
            device_id: "dev-003".into(),
            metric_name: "memory".into(),
            value: 128.0 + i as f64 * 2.5,
            unit: "KB".into(),
            timestamp: chrono::Utc::now(),
        })
        .collect();

    rsx! {
        div {
            class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8",
            div {
                class: "mb-6",
                h1 {
                    class: "text-2xl font-bold text-gray-900",
                    "Metrics"
                }
                p {
                    class: "mt-1 text-sm text-gray-500",
                    "Real-time metrics from all connected devices"
                }
            }

            // Quick stats
            div {
                class: "grid grid-cols-2 lg:grid-cols-4 gap-4 mb-8",
                QuickStat { label: "Avg Temperature", value: "24.3", unit: "C" }
                QuickStat { label: "Avg Humidity", value: "58.2", unit: "%" }
                QuickStat { label: "Active Streams", value: "12", unit: "" }
                QuickStat { label: "Data Points/min", value: "847", unit: "" }
            }

            // Charts grid
            div {
                class: "grid grid-cols-1 lg:grid-cols-2 gap-6",
                MetricChart {
                    title: "Temperature (Sensor A)".to_string(),
                    entries: temperature,
                    color: Some("#ef4444".to_string()),
                }
                MetricChart {
                    title: "Humidity (Sensor A)".to_string(),
                    entries: humidity,
                    color: Some("#3b82f6".to_string()),
                }
                MetricChart {
                    title: "Pressure (Sensor D)".to_string(),
                    entries: pressure,
                    color: Some("#8b5cf6".to_string()),
                }
                MetricChart {
                    title: "Vibration (Motor B)".to_string(),
                    entries: vibration,
                    color: Some("#f59e0b".to_string()),
                }
                MetricChart {
                    title: "CPU Usage (Gateway C)".to_string(),
                    entries: cpu_usage,
                    color: Some("#10b981".to_string()),
                }
                MetricChart {
                    title: "Memory (Gateway C)".to_string(),
                    entries: memory,
                    color: Some("#ec4899".to_string()),
                }
            }
        }
    }
}

#[component]
fn QuickStat(label: &'static str, value: &'static str, unit: &'static str) -> Element {
    rsx! {
        div {
            class: "bg-white rounded-lg shadow border border-gray-200 p-4 text-center",
            p {
                class: "text-xs font-medium text-gray-500 uppercase tracking-wide",
                "{label}"
            }
            p {
                class: "mt-1",
                span {
                    class: "text-2xl font-bold text-gray-900",
                    "{value}"
                }
                if !unit.is_empty() {
                    span {
                        class: "text-sm text-gray-500 ml-1",
                        "{unit}"
                    }
                }
            }
        }
    }
}
