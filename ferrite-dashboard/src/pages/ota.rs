use crate::api::types::*;
use crate::auth::AuthState;
use crate::components::{ErrorDisplay, Loading};
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn OtaPage() -> Element {
    let auth_state = use_context::<Signal<AuthState>>();
    let poll_tick = crate::hooks::use_poll_tick();
    let reload = use_signal(|| 0u32);

    let campaigns_resource = use_resource(move || async move {
        let _tick = poll_tick();
        let _r = reload();
        let client = crate::api::client::authenticated_client(&auth_state());
        client.list_campaigns().await
    });

    let firmware_resource = use_resource(move || async move {
        let _r = reload();
        let client = crate::api::client::authenticated_client(&auth_state());
        client.list_firmware().await
    });

    let campaigns_binding = campaigns_resource.read();
    let firmware_binding = firmware_resource.read();

    let campaigns = match &*campaigns_binding {
        None => return rsx! { Loading {} },
        Some(Err(e)) => return rsx! { ErrorDisplay { message: e.to_string() } },
        Some(Ok(c)) => c.clone(),
    };

    let firmware_list: Vec<FirmwareArtifact> = match &*firmware_binding {
        Some(Ok(f)) => f.clone(),
        _ => vec![],
    };

    let active_count = campaigns.iter().filter(|c| c.status == "active").count();
    let completed_count = campaigns.iter().filter(|c| c.status == "completed").count();

    rsx! {
        div {
            class: "p-6 lg:p-8 max-w-[1400px] mx-auto",

            // Header
            div {
                class: "mb-6 animate-fade-in flex items-start justify-between",
                div {
                    h1 { class: "text-2xl font-semibold text-gray-100", "OTA Campaigns" }
                    p { class: "mt-1 text-sm text-gray-500", "Manage firmware rollouts across your fleet" }
                }
                Link {
                    to: Route::CampaignNew {},
                    class: "inline-flex items-center gap-2 px-4 py-2 bg-ferrite-600 text-white rounded-lg hover:bg-ferrite-500 text-sm font-medium transition-colors",
                    svg {
                        class: "h-4 w-4",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path { stroke_linecap: "round", stroke_linejoin: "round", d: "M12 4v16m8-8H4" }
                    }
                    "New Campaign"
                }
            }

            // Summary cards
            div {
                class: "grid grid-cols-1 sm:grid-cols-3 gap-4 mb-6",
                StatCard { label: "Total Campaigns", value: format!("{}", campaigns.len()), color: "ferrite" }
                StatCard { label: "Active", value: format!("{}", active_count), color: "green" }
                StatCard { label: "Completed", value: format!("{}", completed_count), color: "blue" }
            }

            // Campaigns table
            div {
                class: "bg-surface-900 rounded-xl border border-surface-700 mb-8",
                div {
                    class: "px-5 py-4 border-b border-surface-700 flex items-center justify-between",
                    h2 { class: "text-sm font-semibold text-gray-200", "Campaigns" }
                    p {
                        class: "text-[10px] font-mono text-gray-600 uppercase tracking-wider",
                        "{campaigns.len()} campaign(s)"
                    }
                }
                if campaigns.is_empty() {
                    div {
                        class: "p-12 text-center",
                        p { class: "text-sm text-gray-500 mb-4", "No campaigns yet" }
                        Link {
                            to: Route::CampaignNew {},
                            class: "inline-flex items-center px-4 py-2 bg-ferrite-600 text-white rounded-lg hover:bg-ferrite-500 text-sm font-medium transition-colors",
                            "Create your first campaign"
                        }
                    }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "w-full text-sm",
                            thead {
                                tr { class: "border-b border-surface-700",
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Name" }
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Version" }
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Strategy" }
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Status" }
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Actions" }
                                }
                            }
                            tbody {
                                for campaign in campaigns.iter() {
                                    CampaignRow {
                                        campaign: campaign.clone(),
                                        auth_state,
                                        reload,
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Firmware artifacts
            div {
                class: "bg-surface-900 rounded-xl border border-surface-700",
                div {
                    class: "px-5 py-4 border-b border-surface-700",
                    h2 { class: "text-sm font-semibold text-gray-200", "Firmware Artifacts" }
                    p { class: "mt-0.5 text-xs text-gray-500", "Upload new firmware via the ferrite-cli or API" }
                }
                if firmware_list.is_empty() {
                    div {
                        class: "p-8 text-center",
                        p { class: "text-sm text-gray-500", "No firmware uploaded yet" }
                        p { class: "text-xs text-gray-600 font-mono mt-2",
                            "ferrite firmware upload --file firmware.bin --version 1.0.0"
                        }
                    }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "w-full text-sm",
                            thead {
                                tr { class: "border-b border-surface-700",
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Version" }
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Build ID" }
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Size" }
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "SHA-256" }
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Uploaded" }
                                }
                            }
                            tbody {
                                for artifact in firmware_list.iter() {
                                    tr { class: "border-b border-surface-800 hover:bg-surface-800/50 transition-colors",
                                        td { class: "px-5 py-3 font-mono text-ferrite-400 font-semibold", "{artifact.version}" }
                                        td { class: "px-5 py-3 font-mono text-gray-400 text-xs", "#{artifact.build_id}" }
                                        td { class: "px-5 py-3 text-gray-300", "{artifact.size_display()}" }
                                        td { class: "px-5 py-3 font-mono text-gray-500 text-xs truncate max-w-[140px]",
                                            title: "{artifact.sha256}",
                                            "{&artifact.sha256[..12]}…"
                                        }
                                        td { class: "px-5 py-3 text-gray-500 text-xs", "{fmt_date(&artifact.created_at)}" }
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

// ── Sub-components ────────────────────────────────────────────────────────────

#[component]
fn CampaignRow(
    campaign: OtaCampaign,
    auth_state: Signal<AuthState>,
    reload: Signal<u32>,
) -> Element {
    let id = campaign.id;

    rsx! {
        tr { class: "border-b border-surface-800 hover:bg-surface-800/50 transition-colors",
            td { class: "px-5 py-3",
                Link {
                    to: Route::CampaignDetail { id: id.to_string() },
                    class: "text-ferrite-400 hover:text-ferrite-300 font-medium transition-colors",
                    "{campaign.name}"
                }
            }
            td { class: "px-5 py-3 font-mono text-gray-300 text-xs", "{campaign.target_version}" }
            td { class: "px-5 py-3",
                span {
                    class: "inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium border {campaign.strategy_color()}",
                    "{campaign.strategy}"
                }
            }
            td { class: "px-5 py-3",
                span {
                    class: "inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium border {campaign.status_color()}",
                    "{campaign.status}"
                }
            }
            td { class: "px-5 py-3",
                div { class: "flex items-center gap-2",
                    if campaign.can_activate() {
                        button {
                            class: "px-2.5 py-1 text-xs font-medium bg-green-500/10 border border-green-500/20 text-green-400 rounded-lg hover:bg-green-500/20 transition-colors",
                            onclick: move |_| {
                                let client = crate::api::client::authenticated_client(&auth_state());
                                spawn(async move {
                                    let _ = client.activate_campaign(id).await;
                                    reload.set(reload() + 1);
                                });
                            },
                            "Activate"
                        }
                    }
                    if campaign.can_pause() {
                        button {
                            class: "px-2.5 py-1 text-xs font-medium bg-yellow-500/10 border border-yellow-500/20 text-yellow-400 rounded-lg hover:bg-yellow-500/20 transition-colors",
                            onclick: move |_| {
                                let client = crate::api::client::authenticated_client(&auth_state());
                                spawn(async move {
                                    let _ = client.pause_campaign(id).await;
                                    reload.set(reload() + 1);
                                });
                            },
                            "Pause"
                        }
                    }
                    if campaign.can_rollback() {
                        button {
                            class: "px-2.5 py-1 text-xs font-medium bg-red-500/10 border border-red-500/20 text-red-400 rounded-lg hover:bg-red-500/20 transition-colors",
                            onclick: move |_| {
                                let client = crate::api::client::authenticated_client(&auth_state());
                                spawn(async move {
                                    let _ = client.rollback_campaign(id).await;
                                    reload.set(reload() + 1);
                                });
                            },
                            "Rollback"
                        }
                    }
                    Link {
                        to: Route::CampaignDetail { id: id.to_string() },
                        class: "px-2.5 py-1 text-xs font-medium bg-surface-700 border border-surface-600 text-gray-400 rounded-lg hover:bg-surface-600 transition-colors",
                        "Details"
                    }
                }
            }
        }
    }
}

#[component]
fn StatCard(label: String, value: String, color: String) -> Element {
    let accent = match color.as_str() {
        "green" => "text-green-400",
        "blue" => "text-blue-400",
        "amber" => "text-amber-400",
        "red" => "text-red-400",
        _ => "text-ferrite-400",
    };
    rsx! {
        div {
            class: "bg-surface-900 rounded-xl border border-surface-700 p-4",
            p { class: "text-[10px] font-semibold text-gray-500 uppercase tracking-wider mb-1", "{label}" }
            p { class: "text-2xl font-bold font-mono {accent}", "{value}" }
        }
    }
}

fn fmt_date(s: &str) -> &str {
    s.get(..10).unwrap_or(s)
}
