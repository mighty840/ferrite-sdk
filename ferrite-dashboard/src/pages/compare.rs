use crate::api::types::*;
use crate::auth::AuthState;
use crate::components::{ErrorDisplay, Loading, MetricChart};
use dioxus::prelude::*;

/// Device comparison page — select 2-3 devices to compare metrics side-by-side.
#[component]
pub fn ComparePage() -> Element {
    let auth_state = use_context::<Signal<AuthState>>();
    let mut selected_ids = use_signal(|| Vec::<String>::new());

    let devices_resource = use_resource(move || async move {
        let client = crate::api::client::authenticated_client(&auth_state());
        client.list_devices().await
    });

    let selected = selected_ids();
    let metrics_resource = use_resource(move || {
        let ids = selected.clone();
        async move {
            if ids.is_empty() {
                return Ok(Vec::new());
            }
            let client = crate::api::client::authenticated_client(&auth_state());
            let mut all_metrics = Vec::new();
            for id in &ids {
                match client.list_device_metrics(id).await {
                    Ok(m) => all_metrics.extend(m),
                    Err(e) => return Err(e),
                }
            }
            Ok(all_metrics)
        }
    });

    let binding = devices_resource.read();
    match &*binding {
        None => rsx! { Loading {} },
        Some(Err(e)) => rsx! { ErrorDisplay { message: e.to_string() } },
        Some(Ok(devices)) => {
            let current_selected = selected_ids();

            rsx! {
                div {
                    class: "p-6 lg:p-8 max-w-[1400px] mx-auto",
                    div {
                        class: "mb-6 animate-fade-in",
                        h1 {
                            class: "text-2xl font-semibold text-gray-100",
                            "Compare Devices"
                        }
                        p {
                            class: "mt-1 text-sm text-gray-500",
                            "Select 2-3 devices to compare their metrics side-by-side"
                        }
                    }

                    // Device selector
                    div {
                        class: "bg-surface-900 rounded-xl border border-surface-700 p-4 mb-6",
                        p {
                            class: "text-[10px] font-semibold text-gray-500 uppercase tracking-widest mb-3",
                            "Select Devices ({current_selected.len()}/3)"
                        }
                        div {
                            class: "flex flex-wrap gap-2",
                            for device in devices.iter() {
                                {
                                    let is_selected = current_selected.contains(&device.device_id);
                                    let device_id = device.device_id.clone();
                                    let name = device.display_name();
                                    let can_select = current_selected.len() < 3 || is_selected;

                                    rsx! {
                                        button {
                                            class: if is_selected {
                                                "px-3 py-1.5 rounded-lg text-xs font-mono font-medium bg-ferrite-600 text-white border border-ferrite-500 transition-colors"
                                            } else if can_select {
                                                "px-3 py-1.5 rounded-lg text-xs font-mono font-medium bg-surface-800 text-gray-400 border border-surface-700 hover:border-ferrite-500/40 transition-colors"
                                            } else {
                                                "px-3 py-1.5 rounded-lg text-xs font-mono font-medium bg-surface-800 text-gray-600 border border-surface-700 cursor-not-allowed opacity-50"
                                            },
                                            disabled: !can_select,
                                            onclick: move |_| {
                                                let mut ids = selected_ids();
                                                if let Some(pos) = ids.iter().position(|id| *id == device_id) {
                                                    ids.remove(pos);
                                                } else if ids.len() < 3 {
                                                    ids.push(device_id.clone());
                                                }
                                                selected_ids.set(ids);
                                            },
                                            "{name}"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Comparison view
                    if current_selected.is_empty() {
                        div {
                            class: "bg-surface-900 rounded-xl border border-surface-700 p-12 text-center",
                            p {
                                class: "text-gray-500 text-sm",
                                "Select devices above to compare their metrics"
                            }
                        }
                    } else {
                        {
                            let metrics_binding = metrics_resource.read();
                            match &*metrics_binding {
                                None => rsx! { Loading {} },
                                Some(Err(e)) => rsx! { ErrorDisplay { message: e.to_string() } },
                                Some(Ok(all_metrics)) => {
                                    // Find common metric keys across selected devices
                                    let mut keys: Vec<String> = all_metrics.iter().map(|m| m.key.clone()).collect();
                                    keys.sort();
                                    keys.dedup();

                                    rsx! {
                                        if keys.is_empty() {
                                            div {
                                                class: "bg-surface-900 rounded-xl border border-surface-700 p-12 text-center",
                                                p {
                                                    class: "text-gray-500 text-sm",
                                                    "No metrics found for selected devices"
                                                }
                                            }
                                        } else {
                                            // One chart per metric key, showing all selected devices
                                            for key in &keys {
                                                div {
                                                    class: "mb-6",
                                                    h3 {
                                                        class: "text-sm font-mono font-semibold text-gray-300 mb-3",
                                                        "{key}"
                                                    }
                                                    div {
                                                        class: "grid grid-cols-1 lg:grid-cols-{current_selected.len()} gap-4",
                                                        for device_id in &current_selected {
                                                            {
                                                                let device_metrics: Vec<MetricRow> = all_metrics
                                                                    .iter()
                                                                    .filter(|m| m.device_id == *device_id)
                                                                    .cloned()
                                                                    .collect();

                                                                rsx! {
                                                                    div {
                                                                        class: "bg-surface-900 rounded-xl border border-surface-700 p-4",
                                                                        p {
                                                                            class: "text-[10px] font-mono text-gray-500 mb-2",
                                                                            "{device_id}"
                                                                        }
                                                                        MetricChart {
                                                                            metrics: device_metrics,
                                                                            metric_key: key.clone(),
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
                        }
                    }
                }
            }
        }
    }
}
