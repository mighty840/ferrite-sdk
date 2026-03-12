use crate::api::types::*;
use crate::auth::AuthState;
use crate::components::{ErrorDisplay, Loading};
use crate::Route;
use dioxus::prelude::*;

/// Fleet overview page — shows device groups with status summaries.
#[component]
pub fn FleetPage() -> Element {
    let auth_state = use_context::<Signal<AuthState>>();

    let devices_resource = use_resource(move || async move {
        let client = crate::api::client::authenticated_client(&auth_state());
        client.list_devices().await
    });

    let binding = devices_resource.read();
    match &*binding {
        None => rsx! { Loading {} },
        Some(Err(e)) => rsx! { ErrorDisplay { message: e.to_string() } },
        Some(Ok(devices)) => {
            let total = devices.len();
            let online = devices.iter().filter(|d| d.status_str() == "online").count();
            let offline = devices.iter().filter(|d| d.status_str() == "offline").count();
            let provisioned = devices.iter().filter(|d| d.status_str() == "provisioned").count();
            let other = total - online - offline - provisioned;

            // Group by tags
            let mut tag_groups: std::collections::BTreeMap<String, Vec<&Device>> = std::collections::BTreeMap::new();
            let mut untagged = Vec::new();
            for device in devices.iter() {
                let tags = device.tags_list();
                if tags.is_empty() {
                    untagged.push(device);
                } else {
                    for tag in &tags {
                        tag_groups.entry(tag.clone()).or_default().push(device);
                    }
                }
            }

            rsx! {
                div {
                    class: "p-6 lg:p-8 max-w-[1400px] mx-auto",
                    div {
                        class: "mb-6 animate-fade-in",
                        h1 {
                            class: "text-2xl font-semibold text-gray-100",
                            "Fleet Overview"
                        }
                        p {
                            class: "mt-1 text-sm text-gray-500",
                            "Device fleet status at a glance"
                        }
                    }

                    // Status summary cards
                    div {
                        class: "grid grid-cols-2 lg:grid-cols-5 gap-4 mb-8",
                        FleetStat { label: "Total", value: total.to_string(), color: "text-gray-100" }
                        FleetStat { label: "Online", value: online.to_string(), color: "text-emerald-400" }
                        FleetStat { label: "Offline", value: offline.to_string(), color: "text-red-400" }
                        FleetStat { label: "Provisioned", value: provisioned.to_string(), color: "text-blue-400" }
                        FleetStat { label: "Other", value: other.to_string(), color: "text-amber-400" }
                    }

                    // Health bar
                    if total > 0 {
                        div {
                            class: "mb-8",
                            p {
                                class: "text-[10px] font-semibold text-gray-500 uppercase tracking-widest mb-2",
                                "Fleet Health"
                            }
                            div {
                                class: "h-3 rounded-full bg-surface-800 overflow-hidden flex",
                                if online > 0 {
                                    div {
                                        class: "bg-emerald-500 h-full transition-all",
                                        style: "width: {online as f64 / total as f64 * 100.0}%",
                                    }
                                }
                                if provisioned > 0 {
                                    div {
                                        class: "bg-blue-500 h-full transition-all",
                                        style: "width: {provisioned as f64 / total as f64 * 100.0}%",
                                    }
                                }
                                if offline > 0 {
                                    div {
                                        class: "bg-red-500 h-full transition-all",
                                        style: "width: {offline as f64 / total as f64 * 100.0}%",
                                    }
                                }
                            }
                        }
                    }

                    // Device grid grouped by tags
                    if !tag_groups.is_empty() {
                        for (tag, group_devices) in &tag_groups {
                            div {
                                class: "mb-6",
                                div {
                                    class: "flex items-center space-x-2 mb-3",
                                    span {
                                        class: "inline-flex items-center px-2.5 py-0.5 rounded-md text-xs font-mono font-medium bg-ferrite-500/10 text-ferrite-400 border border-ferrite-500/20",
                                        "{tag}"
                                    }
                                    span {
                                        class: "text-xs text-gray-600",
                                        "{group_devices.len()} device(s)"
                                    }
                                }
                                div {
                                    class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-6 gap-3",
                                    for device in group_devices {
                                        FleetDeviceTile { device: (*device).clone() }
                                    }
                                }
                            }
                        }
                    }

                    if !untagged.is_empty() {
                        div {
                            class: "mb-6",
                            div {
                                class: "flex items-center space-x-2 mb-3",
                                span {
                                    class: "text-xs font-mono text-gray-500 uppercase tracking-wider",
                                    "Untagged"
                                }
                                span {
                                    class: "text-xs text-gray-600",
                                    "{untagged.len()} device(s)"
                                }
                            }
                            div {
                                class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-6 gap-3",
                                for device in &untagged {
                                    FleetDeviceTile { device: (*device).clone() }
                                }
                            }
                        }
                    }

                    if devices.is_empty() {
                        div {
                            class: "bg-surface-900 rounded-xl border border-surface-700 p-12 text-center",
                            p {
                                class: "text-gray-500 text-sm",
                                "No devices in fleet"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FleetStat(label: String, value: String, color: String) -> Element {
    rsx! {
        div {
            class: "bg-surface-900 rounded-xl border border-surface-700 p-4",
            p {
                class: "text-[10px] font-semibold text-gray-500 uppercase tracking-widest mb-2",
                "{label}"
            }
            span {
                class: "text-2xl font-mono font-bold {color}",
                "{value}"
            }
        }
    }
}

#[component]
fn FleetDeviceTile(device: Device) -> Element {
    let status = device.status_str();
    let (dot_color, border_color) = match status {
        "online" => ("bg-emerald-500", "border-emerald-500/20"),
        "offline" => ("bg-red-500", "border-red-500/20"),
        "provisioned" => ("bg-blue-500", "border-blue-500/20"),
        "degraded" => ("bg-amber-500", "border-amber-500/20"),
        _ => ("bg-gray-500", "border-surface-700"),
    };
    let name = device.display_name();
    let device_id = device.device_id.clone();

    rsx! {
        Link {
            to: Route::DeviceDetail { id: device_id },
            class: "bg-surface-900 rounded-lg border {border_color} p-3 hover:bg-surface-850 transition-colors block",
            div {
                class: "flex items-center space-x-2 mb-1",
                div { class: "h-2 w-2 rounded-full {dot_color} flex-shrink-0" }
                span {
                    class: "text-xs font-mono text-gray-200 truncate",
                    "{name}"
                }
            }
            span {
                class: "text-[10px] text-gray-600 font-mono",
                "{device.firmware_version}"
            }
        }
    }
}
