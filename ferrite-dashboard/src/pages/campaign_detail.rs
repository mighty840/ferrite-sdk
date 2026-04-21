use crate::api::types::*;
use crate::auth::AuthState;
use crate::components::{ErrorDisplay, Loading};
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn CampaignDetailPage(id: String) -> Element {
    let auth_state = use_context::<Signal<AuthState>>();
    let poll_tick = crate::hooks::use_poll_tick();
    let mut reload = use_signal(|| 0u32);

    let id_clone = id.clone();
    let campaign_id: i64 = id.parse().unwrap_or(0);

    let summary_resource = use_resource(move || {
        let id_str = id_clone.clone();
        async move {
            let _tick = poll_tick();
            let _r = reload();
            let cid: i64 = id_str.parse().unwrap_or(0);
            let client = crate::api::client::authenticated_client(&auth_state());
            client.get_campaign(cid).await
        }
    });

    let devices_resource = use_resource(move || async move {
        let _tick = poll_tick();
        let _r = reload();
        let client = crate::api::client::authenticated_client(&auth_state());
        client.list_campaign_devices(campaign_id).await
    });

    let summary_binding = summary_resource.read();
    let summary = match &*summary_binding {
        None => return rsx! { Loading {} },
        Some(Err(e)) => return rsx! { ErrorDisplay { message: e.to_string() } },
        Some(Ok(s)) => s.clone(),
    };

    let devices: Vec<CampaignDevice> = match &*devices_resource.read() {
        Some(Ok(d)) => d.clone(),
        _ => vec![],
    };

    let camp = &summary.campaign;
    let total = summary.total_devices();
    let pct = summary.progress_pct();

    rsx! {
        div {
            class: "p-6 lg:p-8 max-w-[1400px] mx-auto",

            // Breadcrumb
            div {
                class: "flex items-center gap-2 text-sm text-gray-500 mb-5",
                Link { to: Route::Ota {}, class: "hover:text-gray-300 transition-colors", "OTA" }
                span { "/" }
                span { class: "text-gray-300", "{camp.name}" }
            }

            // Campaign header
            div {
                class: "bg-surface-900 rounded-xl border border-surface-700 p-5 mb-6 animate-fade-in",
                div {
                    class: "flex flex-col sm:flex-row sm:items-start justify-between gap-4",
                    div {
                        div { class: "flex items-center gap-3 mb-3",
                            h1 { class: "text-xl font-semibold text-gray-100", "{camp.name}" }
                            span {
                                class: "inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium border {camp.status_color()}",
                                "{camp.status}"
                            }
                            span {
                                class: "inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium border {camp.strategy_color()}",
                                "{camp.strategy}"
                            }
                        }
                        div { class: "flex flex-wrap gap-4 text-sm",
                            InfoChip { label: "Target version", value: camp.target_version.clone() }
                            InfoChip { label: "Rollout", value: format!("{}%", camp.rollout_percent) }
                            InfoChip { label: "Created", value: camp.created_at[..10].to_string() }
                        }
                    }
                    // Action buttons
                    div { class: "flex items-center gap-2 flex-shrink-0",
                        if camp.can_activate() {
                            button {
                                class: "px-3 py-1.5 text-sm font-medium bg-green-500/10 border border-green-500/20 text-green-400 rounded-lg hover:bg-green-500/20 transition-colors",
                                onclick: move |_| {
                                    let client = crate::api::client::authenticated_client(&auth_state());
                                    spawn(async move {
                                        let _ = client.activate_campaign(campaign_id).await;
                                        reload.set(reload() + 1);
                                    });
                                },
                                "Activate"
                            }
                        }
                        if camp.can_pause() {
                            button {
                                class: "px-3 py-1.5 text-sm font-medium bg-yellow-500/10 border border-yellow-500/20 text-yellow-400 rounded-lg hover:bg-yellow-500/20 transition-colors",
                                onclick: move |_| {
                                    let client = crate::api::client::authenticated_client(&auth_state());
                                    spawn(async move {
                                        let _ = client.pause_campaign(campaign_id).await;
                                        reload.set(reload() + 1);
                                    });
                                },
                                "Pause"
                            }
                        }
                        if camp.can_rollback() {
                            button {
                                class: "px-3 py-1.5 text-sm font-medium bg-red-500/10 border border-red-500/20 text-red-400 rounded-lg hover:bg-red-500/20 transition-colors",
                                onclick: move |_| {
                                    let client = crate::api::client::authenticated_client(&auth_state());
                                    spawn(async move {
                                        let _ = client.rollback_campaign(campaign_id).await;
                                        reload.set(reload() + 1);
                                    });
                                },
                                "Rollback"
                            }
                        }
                    }
                }
            }

            // Progress summary
            div {
                class: "grid grid-cols-2 sm:grid-cols-4 gap-4 mb-6",
                ProgressCard { label: "Pending", value: summary.pending, color: "gray" }
                ProgressCard { label: "Downloading", value: summary.downloading, color: "blue" }
                ProgressCard { label: "Installed", value: summary.installed, color: "green" }
                ProgressCard { label: "Failed", value: summary.failed, color: "red" }
            }

            // Progress bar
            if total > 0 {
                div {
                    class: "bg-surface-900 rounded-xl border border-surface-700 p-4 mb-6",
                    div { class: "flex items-center justify-between mb-2",
                        p { class: "text-xs text-gray-400 font-medium", "Installation progress" }
                        p { class: "text-xs font-mono text-ferrite-400 font-semibold", "{pct}% ({summary.installed}/{total})" }
                    }
                    div { class: "w-full bg-surface-700 rounded-full h-2",
                        div {
                            class: "bg-ferrite-500 h-2 rounded-full transition-all duration-500",
                            style: "width: {pct}%",
                        }
                    }
                }
            }

            // Device table
            div {
                class: "bg-surface-900 rounded-xl border border-surface-700",
                div {
                    class: "px-5 py-4 border-b border-surface-700 flex items-center justify-between",
                    h2 { class: "text-sm font-semibold text-gray-200", "Device Progress" }
                    p {
                        class: "text-[10px] font-mono text-gray-600 uppercase tracking-wider",
                        "{devices.len()} device(s)"
                    }
                }
                if devices.is_empty() {
                    div { class: "p-8 text-center",
                        p { class: "text-sm text-gray-500", "No devices enrolled in this campaign" }
                    }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "w-full text-sm",
                            thead {
                                tr { class: "border-b border-surface-700",
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Device ID" }
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Status" }
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Last Updated" }
                                }
                            }
                            tbody {
                                for device in devices.iter() {
                                    tr { class: "border-b border-surface-800 hover:bg-surface-800/50 transition-colors",
                                        td { class: "px-5 py-3 font-mono text-gray-200 text-xs",
                                            Link {
                                                to: Route::DeviceDetail { id: device.device_id.clone() },
                                                class: "hover:text-ferrite-400 transition-colors",
                                                "{device.device_id}"
                                            }
                                        }
                                        td { class: "px-5 py-3",
                                            span {
                                                class: "inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium border {device.status_color()}",
                                                "{device.status}"
                                            }
                                        }
                                        td { class: "px-5 py-3 text-gray-500 text-xs font-mono",
                                            "{device.updated_at.get(..16).unwrap_or(&device.updated_at)}"
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
fn InfoChip(label: String, value: String) -> Element {
    rsx! {
        div {
            span { class: "text-gray-500 text-xs", "{label}: " }
            span { class: "text-gray-300 text-xs font-mono", "{value}" }
        }
    }
}

#[component]
fn ProgressCard(label: String, value: i64, color: String) -> Element {
    let accent = match color.as_str() {
        "green" => "text-green-400",
        "blue" => "text-blue-400",
        "red" => "text-red-400",
        _ => "text-gray-400",
    };
    rsx! {
        div {
            class: "bg-surface-900 rounded-xl border border-surface-700 p-4",
            p { class: "text-[10px] font-semibold text-gray-500 uppercase tracking-wider mb-1", "{label}" }
            p { class: "text-2xl font-bold font-mono {accent}", "{value}" }
        }
    }
}
