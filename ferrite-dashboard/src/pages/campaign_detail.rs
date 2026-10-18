use crate::api::types::*;
use crate::auth::AuthState;
use crate::components::{ErrorDisplay, ExportButtons, Loading};
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

    // Update reach (#78): who runs the target build, time to patch.
    let reach_resource = use_resource(move || async move {
        let _tick = poll_tick();
        let _r = reload();
        let client = crate::api::client::authenticated_client(&auth_state());
        let reach = client.get_campaign_reach(campaign_id).await?;
        let csv = client
            .get_text(&format!("/reports/campaigns/{campaign_id}?format=csv"))
            .await?;
        Ok::<_, ApiError>((reach.0, reach.1, csv))
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

    let reach = match &*reach_resource.read() {
        Some(Ok(r)) => Some(r.clone()),
        _ => None,
    };
    let reach_by_device = |id: &str| {
        reach
            .as_ref()
            .and_then(|(r, _, _)| r.devices.iter().find(|d| d.device_id == id).cloned())
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

            // Update reach
            if let Some((r, json, csv)) = reach.clone() {
                div {
                    class: "bg-surface-900 rounded-xl border border-surface-700 p-5 mb-6",
                    div { class: "flex items-center justify-between mb-4",
                        div {
                            h2 { class: "text-sm font-semibold text-gray-200", "Update reach" }
                            p { class: "text-xs text-gray-500 mt-0.5",
                                "Devices reporting target build {r.target_build_id}"
                                if let Some(at) = &r.activated_at { " · activated {at}" }
                            }
                        }
                        ExportButtons {
                            csv_data: csv,
                            json_data: json,
                            filename_base: format!("campaign-{campaign_id}-reach"),
                        }
                    }
                    div { class: "grid grid-cols-2 sm:grid-cols-4 gap-4",
                        ReachStat {
                            label: "On target build",
                            value: format!("{}/{}", r.on_target, r.targeted),
                            hint: format!("{}%", r.percent_on_target),
                        }
                        ReachStat {
                            label: "Unreachable",
                            value: r.unreachable.to_string(),
                            hint: "not on target, offline".to_string(),
                        }
                        ReachStat {
                            label: "Median time to patch",
                            value: r.time_to_patch.median_secs.map(format_duration_secs).unwrap_or_else(|| "–".into()),
                            hint: format!("{} device(s) patched", r.time_to_patch.count),
                        }
                        ReachStat {
                            label: "Slowest",
                            value: r.time_to_patch.max_secs.map(format_duration_secs).unwrap_or_else(|| "–".into()),
                            hint: "activation → first report of target".to_string(),
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
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Running build" }
                                    th { class: "px-5 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Time to patch" }
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
                                        {
                                            let rd = reach_by_device(&device.device_id);
                                            let build = rd.as_ref().and_then(|d| d.current_build_id).map(|b| b.to_string()).unwrap_or_else(|| "–".into());
                                            let on_target = rd.as_ref().is_some_and(|d| d.on_target);
                                            let unreachable = rd.as_ref().is_some_and(|d| !d.reachable && !d.on_target);
                                            let ttp = match &rd {
                                                Some(d) if d.patched_before_campaign => "already on target".to_string(),
                                                Some(d) => d.time_to_patch_secs.map(format_duration_secs).unwrap_or_else(|| "–".into()),
                                                None => "–".into(),
                                            };
                                            rsx! {
                                                td { class: "px-5 py-3 font-mono text-xs",
                                                    span { class: if on_target { "text-green-400" } else { "text-gray-300" }, "{build}" }
                                                    if unreachable {
                                                        span { class: "ml-2 text-[10px] text-yellow-500", "unreachable" }
                                                    }
                                                }
                                                td { class: "px-5 py-3 text-gray-400 text-xs font-mono", "{ttp}" }
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

#[component]
fn ReachStat(label: String, value: String, hint: String) -> Element {
    rsx! {
        div {
            class: "bg-surface-800/50 rounded-lg border border-surface-700 p-3",
            p { class: "text-[10px] font-semibold text-gray-500 uppercase tracking-wider mb-1", "{label}" }
            p { class: "text-xl font-bold font-mono text-gray-100", "{value}" }
            p { class: "text-[10px] text-gray-500 mt-1", "{hint}" }
        }
    }
}
