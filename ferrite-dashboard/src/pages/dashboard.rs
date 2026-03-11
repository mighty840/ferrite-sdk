use crate::api::types::*;
use crate::auth::AuthState;
use crate::components::DeviceCard;
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn DashboardPage() -> Element {
    let auth_state = use_context::<Signal<AuthState>>();

    let devices_resource = use_resource(move || async move {
        let api_url = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_else(|| "http://localhost:4000".into());
        let mut client = crate::api::ApiClient::new(&api_url);
        if let AuthState::Authenticated { ref token, .. } = auth_state() {
            client.set_token(token.clone());
        }
        client.list_devices().await
    });

    let faults_resource = use_resource(move || async move {
        let api_url = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_else(|| "http://localhost:4000".into());
        let mut client = crate::api::ApiClient::new(&api_url);
        if let AuthState::Authenticated { ref token, .. } = auth_state() {
            client.set_token(token.clone());
        }
        client.list_faults().await
    });

    let devices = match &*devices_resource.read() {
        Some(Ok(d)) => d.clone(),
        _ => Vec::new(),
    };

    let faults = match &*faults_resource.read() {
        Some(Ok(f)) => f.clone(),
        _ => Vec::new(),
    };

    let online_count = devices.iter().filter(|d| d.status_str() == "online").count();
    let provisioned_count = devices.iter().filter(|d| d.status_str() == "provisioned").count();
    let _offline_count = devices.iter().filter(|d| d.status_str() == "offline").count();
    let total_count = devices.len();
    let fault_count = faults.len();

    let recent_devices: Vec<&Device> = devices.iter().take(4).collect();

    rsx! {
        div {
            class: "p-6 lg:p-8 max-w-[1400px] mx-auto",
            // Header
            div {
                class: "mb-8 animate-fade-in",
                h1 {
                    class: "text-2xl font-semibold text-gray-100",
                    "Overview"
                }
                p {
                    class: "mt-1 text-sm text-gray-500",
                    "Fleet status and recent activity"
                }
            }

            // Stat cards
            div {
                class: "grid grid-cols-2 lg:grid-cols-4 gap-4 mb-8",
                StatCard {
                    label: "Total Devices",
                    value: total_count.to_string(),
                    accent: "text-gray-300",
                    bg: "bg-surface-800",
                    border: "border-surface-700",
                }
                StatCard {
                    label: "Online",
                    value: online_count.to_string(),
                    accent: "text-emerald-400",
                    bg: "bg-emerald-500/5",
                    border: "border-emerald-500/20",
                }
                StatCard {
                    label: "Provisioned",
                    value: provisioned_count.to_string(),
                    accent: "text-blue-400",
                    bg: "bg-blue-500/5",
                    border: "border-blue-500/20",
                }
                StatCard {
                    label: "Faults",
                    value: fault_count.to_string(),
                    accent: "text-red-400",
                    bg: "bg-red-500/5",
                    border: "border-red-500/20",
                }
            }

            // Devices section
            div {
                class: "mb-8",
                div {
                    class: "flex items-center justify-between mb-4",
                    h2 {
                        class: "text-sm font-medium text-gray-400 uppercase tracking-wider",
                        "Devices"
                    }
                    Link {
                        to: Route::Devices {},
                        class: "text-xs text-ferrite-500 hover:text-ferrite-400 font-medium transition-colors",
                        "View all"
                    }
                }
                if devices.is_empty() {
                    div {
                        class: "bg-surface-900 rounded-xl border border-surface-700 p-8 text-center",
                        p {
                            class: "text-gray-500 text-sm",
                            if devices_resource.read().is_none() {
                                "Loading devices..."
                            } else {
                                "No devices registered yet"
                            }
                        }
                    }
                } else {
                    div {
                        class: "grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-4 gap-4",
                        for device in recent_devices {
                            DeviceCard { device: device.clone() }
                        }
                    }
                }
            }

            // Recent faults
            div {
                class: "bg-surface-900 rounded-xl border border-surface-700",
                div {
                    class: "px-5 py-4 border-b border-surface-700",
                    h2 {
                        class: "text-sm font-medium text-gray-400 uppercase tracking-wider",
                        "Recent Faults"
                    }
                }
                if faults.is_empty() {
                    div {
                        class: "px-5 py-8 text-center text-sm text-gray-500",
                        "No faults recorded"
                    }
                } else {
                    div {
                        class: "divide-y divide-surface-750",
                        for fault in faults.iter().take(5) {
                            FaultRow { fault: fault.clone() }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StatCard(
    label: &'static str,
    value: String,
    accent: &'static str,
    bg: &'static str,
    border: &'static str,
) -> Element {
    rsx! {
        div {
            class: "rounded-xl border p-5 {bg} {border} animate-fade-in",
            p {
                class: "text-[10px] font-semibold text-gray-500 uppercase tracking-widest mb-2",
                "{label}"
            }
            p {
                class: "text-3xl font-mono font-bold {accent}",
                "{value}"
            }
        }
    }
}

#[component]
fn FaultRow(fault: FaultEvent) -> Element {
    let color = match fault.fault_type {
        0 => "bg-red-500",     // HardFault
        1 => "bg-amber-500",   // MemManage
        2 => "bg-orange-500",  // BusFault
        _ => "bg-yellow-500",  // UsageFault
    };
    let type_name = fault.fault_type_name();
    let symbol_display = fault.symbol.as_deref().unwrap_or("unknown");
    let pc_hex = format!("0x{:08X}", fault.pc);

    rsx! {
        div {
            class: "px-5 py-3.5 flex items-center space-x-4 hover:bg-surface-850 transition-colors",
            div {
                class: "flex-shrink-0",
                div { class: "h-2 w-2 rounded-full {color}" }
            }
            div {
                class: "flex-1 min-w-0",
                p {
                    class: "text-sm font-medium text-gray-200",
                    "{type_name}"
                }
                p {
                    class: "text-xs text-gray-500 font-mono",
                    "{fault.device_id} — {symbol_display} ({pc_hex})"
                }
            }
            span {
                class: "text-[10px] text-gray-600 font-mono flex-shrink-0",
                "{fault.created_at}"
            }
        }
    }
}
