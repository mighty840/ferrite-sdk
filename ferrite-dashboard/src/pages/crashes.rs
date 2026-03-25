use crate::api::types::*;
use crate::auth::AuthState;
use crate::components::{ErrorDisplay, Loading};
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn CrashesPage() -> Element {
    let auth_state = use_context::<Signal<AuthState>>();
    let poll_tick = crate::hooks::use_poll_tick();

    let crashes_resource = use_resource(move || async move {
        let _tick = poll_tick();
        let client = crate::api::client::authenticated_client(&auth_state());
        client.list_crash_groups().await
    });

    let binding = crashes_resource.read();
    match &*binding {
        None => rsx! { Loading {} },
        Some(Err(e)) => rsx! { ErrorDisplay { message: e.to_string() } },
        Some(Ok(groups)) => {
            let total_occurrences: i64 = groups.iter().map(|g| g.occurrence_count).sum();
            let total_devices: i64 = groups
                .iter()
                .map(|g| g.affected_device_count)
                .max()
                .unwrap_or(0);

            rsx! {
                div {
                    class: "p-6 lg:p-8 max-w-[1400px] mx-auto",
                    div {
                        class: "mb-6 animate-fade-in",
                        h1 {
                            class: "text-2xl font-semibold text-gray-100",
                            "Crash Analytics"
                        }
                        p {
                            class: "mt-1 text-sm text-gray-500",
                            "Faults grouped by crash signature (PC + fault type)"
                        }
                    }

                    // Summary cards
                    div {
                        class: "grid grid-cols-1 sm:grid-cols-3 gap-4 mb-6",
                        StatCard { label: "Crash Groups", value: format!("{}", groups.len()), color: "ferrite" }
                        StatCard { label: "Total Occurrences", value: format!("{}", total_occurrences), color: "red" }
                        StatCard { label: "Devices Affected", value: format!("{}", total_devices), color: "amber" }
                    }

                    if groups.is_empty() {
                        div {
                            class: "bg-surface-900 rounded-xl border border-surface-700 p-12 text-center",
                            p {
                                class: "text-gray-500 text-sm",
                                "No crashes recorded"
                            }
                        }
                    } else {
                        div {
                            class: "space-y-3",
                            for group in groups {
                                CrashGroupCard { group: group.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StatCard(label: String, value: String, color: String) -> Element {
    let bg = format!("bg-{}-500/5", color);
    let border = format!("border-{}-500/20", color);
    let text = format!("text-{}-400", color);

    rsx! {
        div {
            class: "rounded-xl border p-4 {bg} {border}",
            p {
                class: "text-[10px] font-mono uppercase tracking-wider text-gray-500 mb-1",
                "{label}"
            }
            p {
                class: "text-2xl font-semibold font-mono {text}",
                "{value}"
            }
        }
    }
}

#[component]
fn CrashGroupCard(group: CrashGroup) -> Element {
    let type_name = group.fault_type_name();
    let pc_hex = format!("0x{:08X}", group.pc);
    let title = group.title.as_deref().unwrap_or("unknown symbol");

    let (severity_bg, severity_border, severity_dot) = match group.fault_type {
        0 => ("bg-red-500/5", "border-red-500/20", "bg-red-500"),
        1 | 2 => ("bg-amber-500/5", "border-amber-500/20", "bg-amber-500"),
        _ => ("bg-blue-500/5", "border-blue-500/20", "bg-blue-500"),
    };

    rsx! {
        Link {
            to: Route::CrashDetail { id: group.id.to_string() },
            class: "block bg-surface-900 rounded-xl border {severity_border} p-5 {severity_bg} hover:bg-surface-850 transition-colors cursor-pointer",
            div {
                class: "flex items-start space-x-4",
                div {
                    class: "flex-shrink-0 mt-1",
                    div { class: "h-2.5 w-2.5 rounded-full {severity_dot}" }
                }
                div {
                    class: "flex-1 min-w-0",
                    div {
                        class: "flex items-center justify-between",
                        div {
                            class: "flex items-center space-x-2",
                            h3 {
                                class: "text-sm font-mono font-semibold text-gray-100",
                                "{title}"
                            }
                            span {
                                class: "text-[10px] font-mono text-gray-500",
                                "{type_name} at {pc_hex}"
                            }
                        }
                        div {
                            class: "flex items-center space-x-3",
                            span {
                                class: "inline-flex items-center px-2 py-0.5 rounded text-xs font-mono font-medium bg-red-500/10 text-red-400 border border-red-500/20",
                                "{group.occurrence_count}x"
                            }
                            span {
                                class: "text-[10px] text-gray-600 font-mono",
                                "{group.affected_device_count} device(s)"
                            }
                        }
                    }
                    div {
                        class: "mt-2 flex items-center space-x-4 text-xs text-gray-500 font-mono",
                        span { "First: {group.first_seen}" }
                        span { "Last: {group.last_seen}" }
                    }
                }
            }
        }
    }
}

/// Crash group detail page — shows individual fault occurrences.
#[component]
pub fn CrashDetailPage(id: String) -> Element {
    let auth_state = use_context::<Signal<AuthState>>();
    let group_id: i64 = id.parse().unwrap_or(0);

    let detail_resource = use_resource(move || async move {
        let client = crate::api::client::authenticated_client(&auth_state());
        client.get_crash_group(group_id).await
    });

    let binding = detail_resource.read();
    match &*binding {
        None => rsx! { Loading {} },
        Some(Err(e)) => rsx! { ErrorDisplay { message: e.to_string() } },
        Some(Ok((group, occurrences))) => {
            let type_name = group.fault_type_name();
            let pc_hex = format!("0x{:08X}", group.pc);
            let title = group.title.as_deref().unwrap_or("unknown symbol");

            rsx! {
                div {
                    class: "p-6 lg:p-8 max-w-[1400px] mx-auto",
                    // Back link
                    div {
                        class: "mb-4",
                        Link {
                            to: Route::Crashes {},
                            class: "text-sm text-gray-500 hover:text-ferrite-400 transition-colors",
                            "← Back to Crash Analytics"
                        }
                    }

                    // Header
                    div {
                        class: "mb-6 animate-fade-in",
                        h1 {
                            class: "text-2xl font-semibold text-gray-100 font-mono",
                            "{title}"
                        }
                        p {
                            class: "mt-1 text-sm text-gray-500 font-mono",
                            "{type_name} at {pc_hex}"
                        }
                    }

                    // Stats
                    div {
                        class: "grid grid-cols-1 sm:grid-cols-4 gap-4 mb-6",
                        StatCard { label: "Occurrences", value: format!("{}", group.occurrence_count), color: "red" }
                        StatCard { label: "Devices Affected", value: format!("{}", group.affected_device_count), color: "amber" }
                        StatCard { label: "First Seen", value: group.first_seen.clone(), color: "blue" }
                        StatCard { label: "Last Seen", value: group.last_seen.clone(), color: "ferrite" }
                    }

                    // Occurrences
                    div {
                        class: "mb-4",
                        p {
                            class: "text-[10px] text-gray-600 font-mono uppercase tracking-wider",
                            "{occurrences.len()} occurrence(s)"
                        }
                    }

                    div {
                        class: "space-y-3",
                        for fault in occurrences {
                            OccurrenceCard { fault: fault.clone() }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn OccurrenceCard(fault: FaultEvent) -> Element {
    let pc_hex = format!("0x{:08X}", fault.pc);
    let lr_hex = format!("0x{:08X}", fault.lr);
    let symbol = fault.symbol.as_deref().unwrap_or("unknown");

    rsx! {
        div {
            class: "bg-surface-900 rounded-xl border border-surface-700 p-4",
            div {
                class: "flex items-center justify-between mb-2",
                div {
                    class: "flex items-center space-x-2",
                    span {
                        class: "text-sm font-mono font-medium text-gray-200",
                        "{fault.device_id}"
                    }
                    span {
                        class: "text-[10px] font-mono text-gray-500",
                        "PC {pc_hex} | LR {lr_hex}"
                    }
                }
                span {
                    class: "text-[10px] text-gray-600 font-mono",
                    "{fault.created_at}"
                }
            }
            p {
                class: "text-xs text-gray-400 font-mono",
                "Symbol: {symbol}"
            }
        }
    }
}
