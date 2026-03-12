use crate::auth::AuthState;
use crate::components::{metrics_to_csv, ErrorDisplay, ExportButtons, Loading, MetricChart};
use dioxus::prelude::*;

#[component]
pub fn MetricsPage() -> Element {
    let auth_state = use_context::<Signal<AuthState>>();

    let metrics_resource = use_resource(move || async move {
        let client = crate::api::client::authenticated_client(&auth_state());
        client.list_all_metrics().await
    });

    let binding = metrics_resource.read();
    match &*binding {
        None => rsx! { Loading {} },
        Some(Err(e)) => rsx! { ErrorDisplay { message: e.to_string() } },
        Some(Ok(metrics)) => {
            let total = metrics.len();
            // Group by key
            let mut keys: Vec<String> = metrics.iter().map(|m| m.key.clone()).collect();
            keys.sort();
            keys.dedup();

            rsx! {
                div {
                    class: "p-6 lg:p-8 max-w-[1400px] mx-auto",
                    div {
                        class: "mb-6 animate-fade-in",
                        h1 {
                            class: "text-2xl font-semibold text-gray-100",
                            "Metrics"
                        }
                        p {
                            class: "mt-1 text-sm text-gray-500",
                            "Telemetry data from connected devices"
                        }
                    }

                    // Quick stats + export
                    div {
                        class: "flex items-center justify-between mb-4",
                        div {
                            class: "flex items-center space-x-4",
                            span {
                                class: "text-sm text-gray-400 font-mono",
                                "{total} data points, {keys.len()} keys"
                            }
                        }
                        if !metrics.is_empty() {
                            ExportButtons {
                                csv_data: metrics_to_csv(metrics),
                                json_data: serde_json::to_string_pretty(metrics).unwrap_or_default(),
                                filename_base: "ferrite-metrics".to_string(),
                            }
                        }
                    }

                    div {
                        class: "grid grid-cols-2 lg:grid-cols-4 gap-4 mb-8",
                        QuickStat { label: "Total Data Points", value: total.to_string() }
                        QuickStat { label: "Unique Keys", value: keys.len().to_string() }
                    }

                    if metrics.is_empty() {
                        div {
                            class: "bg-surface-900 rounded-xl border border-surface-700 p-12 text-center",
                            p {
                                class: "text-gray-500 text-sm",
                                "No metrics recorded yet"
                            }
                        }
                    } else {
                        // Charts per metric key
                        div {
                            class: "grid grid-cols-1 lg:grid-cols-2 gap-4 mb-8",
                            for key in &keys {
                                MetricChart {
                                    metrics: metrics.clone(),
                                    metric_key: key.clone(),
                                }
                            }
                        }
                        // Metric table
                        div {
                            class: "bg-surface-900 rounded-xl border border-surface-700 overflow-hidden",
                            div {
                                class: "overflow-x-auto",
                                table {
                                    class: "w-full text-sm",
                                    thead {
                                        class: "bg-surface-850 border-b border-surface-700",
                                        tr {
                                            th { class: "px-4 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Device" }
                                            th { class: "px-4 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Key" }
                                            th { class: "px-4 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Value" }
                                            th { class: "px-4 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Ticks" }
                                            th { class: "px-4 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Time" }
                                        }
                                    }
                                    tbody {
                                        class: "divide-y divide-surface-750",
                                        for metric in metrics.iter().take(100) {
                                            tr {
                                                class: "hover:bg-surface-850 transition-colors",
                                                td { class: "px-4 py-3 text-gray-400 font-mono", "{metric.device_id}" }
                                                td { class: "px-4 py-3 text-gray-200 font-mono", "{metric.key}" }
                                                td { class: "px-4 py-3 text-gray-300 font-mono text-xs", "{metric.value_json}" }
                                                td { class: "px-4 py-3 text-gray-500 font-mono text-xs", "{metric.timestamp_ticks}" }
                                                td { class: "px-4 py-3 text-gray-500 font-mono text-xs", "{metric.created_at}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn QuickStat(label: String, value: String) -> Element {
    rsx! {
        div {
            class: "bg-surface-900 rounded-xl border border-surface-700 p-4",
            p {
                class: "text-[10px] font-semibold text-gray-500 uppercase tracking-widest mb-2",
                "{label}"
            }
            div {
                span {
                    class: "text-2xl font-mono font-bold text-gray-100",
                    "{value}"
                }
            }
        }
    }
}
