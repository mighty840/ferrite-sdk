use crate::api::types::*;
use crate::auth::AuthState;
use crate::components::{ErrorDisplay, Loading};
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn DeviceDetailPage(id: String) -> Element {
    let mut active_tab = use_signal(|| "faults".to_string());
    let auth_state = use_context::<Signal<AuthState>>();
    let device_id = id.clone();

    let devices_resource = use_resource(move || {
        let did = device_id.clone();
        async move {
            let api_url = web_sys::window()
                .and_then(|w| w.location().origin().ok())
                .unwrap_or_else(|| "http://localhost:4000".into());
            let mut client = crate::api::ApiClient::new(&api_url);
            if let AuthState::Authenticated { ref token, .. } = auth_state() {
                client.set_token(token.clone());
            }
            // Fetch device list and find ours
            let devices = client.list_devices().await?;
            devices
                .into_iter()
                .find(|d| d.device_id == did)
                .ok_or(ApiError::NotFound)
        }
    });

    let faults_id = id.clone();
    let faults_resource = use_resource(move || {
        let did = faults_id.clone();
        async move {
            let api_url = web_sys::window()
                .and_then(|w| w.location().origin().ok())
                .unwrap_or_else(|| "http://localhost:4000".into());
            let mut client = crate::api::ApiClient::new(&api_url);
            if let AuthState::Authenticated { ref token, .. } = auth_state() {
                client.set_token(token.clone());
            }
            client.list_device_faults(&did).await
        }
    });

    let metrics_id = id.clone();
    let metrics_resource = use_resource(move || {
        let did = metrics_id.clone();
        async move {
            let api_url = web_sys::window()
                .and_then(|w| w.location().origin().ok())
                .unwrap_or_else(|| "http://localhost:4000".into());
            let mut client = crate::api::ApiClient::new(&api_url);
            if let AuthState::Authenticated { ref token, .. } = auth_state() {
                client.set_token(token.clone());
            }
            client.list_device_metrics(&did).await
        }
    });

    let binding = devices_resource.read();
    match &*binding {
        None => rsx! { Loading {} },
        Some(Err(e)) => rsx! { ErrorDisplay { message: e.to_string() } },
        Some(Ok(device)) => {
            let status = device.status_str().to_string();
            let (status_dot, status_color, status_glow) = match status.as_str() {
                "online" => ("bg-emerald-400", "text-emerald-400", true),
                "offline" => ("bg-red-400", "text-red-400", false),
                "degraded" => ("bg-amber-400", "text-amber-400", false),
                "provisioned" => ("bg-blue-400", "text-blue-400", false),
                _ => ("bg-gray-500", "text-gray-500", false),
            };

            let display_name = device.display_name();
            let key_display = device.key_display();
            let tags = device.tags_list();

            let faults: Vec<FaultEvent> = match &*faults_resource.read() {
                Some(Ok(f)) => f.clone(),
                _ => Vec::new(),
            };

            let metrics: Vec<MetricRow> = match &*metrics_resource.read() {
                Some(Ok(m)) => m.clone(),
                _ => Vec::new(),
            };

            rsx! {
                div {
                    class: "p-6 lg:p-8 max-w-[1400px] mx-auto",
                    // Breadcrumb
                    nav {
                        class: "flex items-center mb-6 text-xs font-mono animate-fade-in",
                        Link {
                            to: Route::Devices {},
                            class: "text-gray-500 hover:text-ferrite-400 transition-colors",
                            "Devices"
                        }
                        span { class: "mx-2 text-gray-700", "/" }
                        span { class: "text-gray-300", "{display_name}" }
                    }

                    // Device header
                    div {
                        class: "bg-surface-900 rounded-xl border border-surface-700 p-6 mb-6 animate-fade-in",
                        div {
                            class: "flex flex-col md:flex-row md:items-center md:justify-between",
                            div {
                                class: "flex items-center space-x-4",
                                div {
                                    class: "h-12 w-12 rounded-xl bg-ferrite-600/10 border border-ferrite-600/20 flex items-center justify-center",
                                    svg {
                                        class: "h-6 w-6 text-ferrite-500",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "1.5",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z"
                                        }
                                    }
                                }
                                div {
                                    h1 {
                                        class: "text-xl font-semibold text-gray-100",
                                        "{display_name}"
                                    }
                                    p {
                                        class: "text-xs text-gray-500 font-mono",
                                        "{device.device_id}"
                                    }
                                }
                            }
                            div {
                                class: "mt-4 md:mt-0 flex items-center space-x-2",
                                div {
                                    class: if status_glow {
                                        "h-2.5 w-2.5 rounded-full {status_dot} pulse-glow"
                                    } else {
                                        "h-2.5 w-2.5 rounded-full {status_dot}"
                                    },
                                }
                                span {
                                    class: "text-xs font-medium {status_color} uppercase tracking-wider font-mono",
                                    "{status}"
                                }
                            }
                        }
                        div {
                            class: "mt-6 grid grid-cols-2 md:grid-cols-4 gap-4",
                            DetailField { label: "Device Key".to_string(), value: key_display }
                            DetailField { label: "Firmware".to_string(), value: device.firmware_version.clone() }
                            DetailField { label: "Last Seen".to_string(), value: device.last_seen.clone() }
                            DetailField { label: "First Seen".to_string(), value: device.first_seen.clone() }
                        }
                        if !tags.is_empty() {
                            div {
                                class: "mt-4 flex flex-wrap gap-1.5",
                                for tag in &tags {
                                    span {
                                        class: "inline-block bg-surface-750 text-gray-400 text-[10px] px-2 py-0.5 rounded font-mono",
                                        "{tag}"
                                    }
                                }
                            }
                        }
                    }

                    // Tabs
                    div {
                        class: "flex items-center space-x-1 mb-6 bg-surface-900 rounded-lg p-1 border border-surface-700 w-fit",
                        TabButton {
                            label: "Faults",
                            tab_id: "faults",
                            active_tab: active_tab(),
                            on_click: move |_| active_tab.set("faults".into()),
                        }
                        TabButton {
                            label: "Metrics",
                            tab_id: "metrics",
                            active_tab: active_tab(),
                            on_click: move |_| active_tab.set("metrics".into()),
                        }
                    }

                    // Tab content
                    match active_tab().as_str() {
                        "faults" => rsx! {
                            if faults.is_empty() {
                                div {
                                    class: "bg-surface-900 rounded-xl border border-surface-700 p-8 text-center text-sm text-gray-500",
                                    "No faults recorded for this device"
                                }
                            } else {
                                div {
                                    class: "space-y-3",
                                    for fault in &faults {
                                        FaultRow { fault: fault.clone() }
                                    }
                                }
                            }
                        },
                        "metrics" => rsx! {
                            if metrics.is_empty() {
                                div {
                                    class: "bg-surface-900 rounded-xl border border-surface-700 p-8 text-center text-sm text-gray-500",
                                    "No metrics recorded for this device"
                                }
                            } else {
                                div {
                                    class: "bg-surface-900 rounded-xl border border-surface-700 overflow-hidden",
                                    table {
                                        class: "w-full text-sm",
                                        thead {
                                            class: "bg-surface-850 border-b border-surface-700",
                                            tr {
                                                th { class: "px-4 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Key" }
                                                th { class: "px-4 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Value" }
                                                th { class: "px-4 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Ticks" }
                                                th { class: "px-4 py-3 text-left text-[10px] font-semibold text-gray-500 uppercase tracking-wider", "Time" }
                                            }
                                        }
                                        tbody {
                                            class: "divide-y divide-surface-750",
                                            for m in metrics.iter().take(50) {
                                                tr {
                                                    class: "hover:bg-surface-850 transition-colors",
                                                    td { class: "px-4 py-3 text-gray-200 font-mono", "{m.key}" }
                                                    td { class: "px-4 py-3 text-gray-300 font-mono text-xs", "{m.value_json}" }
                                                    td { class: "px-4 py-3 text-gray-500 font-mono text-xs", "{m.timestamp_ticks}" }
                                                    td { class: "px-4 py-3 text-gray-500 font-mono text-xs", "{m.created_at}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        _ => rsx! {
                            p { class: "text-gray-500", "Unknown tab" }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn FaultRow(fault: FaultEvent) -> Element {
    let type_name = fault.fault_type_name();
    let (severity_bg, severity_border, severity_dot) = match fault.fault_type {
        0 => ("bg-red-500/5", "border-red-500/20", "bg-red-500"),
        1 | 2 => ("bg-amber-500/5", "border-amber-500/20", "bg-amber-500"),
        _ => ("bg-blue-500/5", "border-blue-500/20", "bg-blue-500"),
    };
    let pc_hex = format!("0x{:08X}", fault.pc);
    let lr_hex = format!("0x{:08X}", fault.lr);
    let symbol = fault.symbol.as_deref().unwrap_or("unknown");

    rsx! {
        div {
            class: "bg-surface-900 rounded-xl border {severity_border} p-5 {severity_bg}",
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
                        h3 {
                            class: "text-sm font-mono font-semibold text-gray-100",
                            "{type_name} at {pc_hex}"
                        }
                        span {
                            class: "text-[10px] text-gray-600 font-mono",
                            "{fault.created_at}"
                        }
                    }
                    p {
                        class: "mt-1.5 text-sm text-gray-400 font-mono",
                        "Symbol: {symbol} | LR: {lr_hex}"
                    }
                }
            }
        }
    }
}

#[component]
fn DetailField(label: String, value: String) -> Element {
    rsx! {
        div {
            p {
                class: "text-[10px] font-semibold text-gray-500 uppercase tracking-widest mb-1",
                "{label}"
            }
            p {
                class: "text-sm text-gray-200 font-mono",
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
        "px-4 py-2 rounded-md text-sm font-medium bg-ferrite-600 text-white cursor-pointer transition-all"
    } else {
        "px-4 py-2 rounded-md text-sm font-medium text-gray-400 hover:text-gray-200 cursor-pointer transition-all"
    };

    rsx! {
        button {
            class: classes,
            onclick: move |e| on_click.call(e),
            "{label}"
        }
    }
}
