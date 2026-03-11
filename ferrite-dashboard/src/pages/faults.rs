use crate::api::types::*;
use crate::auth::AuthState;
use crate::components::{ErrorDisplay, Loading};
use dioxus::prelude::*;

#[component]
pub fn FaultsPage() -> Element {
    let mut type_filter = use_signal(|| "all".to_string());
    let auth_state = use_context::<Signal<AuthState>>();

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

    let binding = faults_resource.read();
    match &*binding {
        None => rsx! { Loading {} },
        Some(Err(e)) => rsx! { ErrorDisplay { message: e.to_string() } },
        Some(Ok(faults)) => {
            let filtered: Vec<&FaultEvent> = faults
                .iter()
                .filter(|f| match type_filter().as_str() {
                    "hardfault" => f.fault_type == 0,
                    "memmanage" => f.fault_type == 1,
                    "busfault" => f.fault_type == 2,
                    "usagefault" => f.fault_type == 3,
                    _ => true,
                })
                .collect();

            let hard_count = faults.iter().filter(|f| f.fault_type == 0).count();
            let other_count = faults.iter().filter(|f| f.fault_type != 0).count();
            let count = filtered.len();

            rsx! {
                div {
                    class: "p-6 lg:p-8 max-w-[1400px] mx-auto",
                    div {
                        class: "mb-6 animate-fade-in",
                        h1 {
                            class: "text-2xl font-semibold text-gray-100",
                            "Faults"
                        }
                        p {
                            class: "mt-1 text-sm text-gray-500",
                            "Device fault events and diagnostics"
                        }
                    }

                    // Summary badges
                    div {
                        class: "flex items-center space-x-3 mb-6",
                        if hard_count > 0 {
                            span {
                                class: "inline-flex items-center px-3 py-1 rounded-lg text-xs font-mono font-medium bg-red-500/10 text-red-400 border border-red-500/20",
                                "{hard_count} HardFault"
                            }
                        }
                        if other_count > 0 {
                            span {
                                class: "inline-flex items-center px-3 py-1 rounded-lg text-xs font-mono font-medium bg-amber-500/10 text-amber-400 border border-amber-500/20",
                                "{other_count} Other"
                            }
                        }
                    }

                    // Filters
                    div {
                        class: "flex flex-col sm:flex-row gap-3 mb-6",
                        select {
                            class: "px-4 py-2.5 bg-surface-900 border border-surface-700 rounded-lg text-sm text-gray-300 focus:ring-2 focus:ring-ferrite-500/40 focus:border-ferrite-600 outline-none transition-all",
                            value: "{type_filter}",
                            onchange: move |e| type_filter.set(e.value()),
                            option { value: "all", "All types" }
                            option { value: "hardfault", "HardFault" }
                            option { value: "memmanage", "MemManage" }
                            option { value: "busfault", "BusFault" }
                            option { value: "usagefault", "UsageFault" }
                        }
                    }

                    p {
                        class: "text-[10px] text-gray-600 mb-4 font-mono uppercase tracking-wider",
                        "{count} fault(s)"
                    }

                    if faults.is_empty() {
                        div {
                            class: "bg-surface-900 rounded-xl border border-surface-700 p-12 text-center",
                            p {
                                class: "text-gray-500 text-sm",
                                "No faults recorded"
                            }
                        }
                    } else {
                        div {
                            class: "space-y-3",
                            for fault in &filtered {
                                FaultCard { fault: (*fault).clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FaultCard(fault: FaultEvent) -> Element {
    let type_name = fault.fault_type_name();
    let (severity_bg, severity_text, severity_border, severity_dot) = match fault.fault_type {
        0 => ("bg-red-500/5", "text-red-400", "border-red-500/20", "bg-red-500"),
        1 | 2 => ("bg-amber-500/5", "text-amber-400", "border-amber-500/20", "bg-amber-500"),
        _ => ("bg-blue-500/5", "text-blue-400", "border-blue-500/20", "bg-blue-500"),
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
                        div {
                            class: "flex items-center space-x-2",
                            h3 {
                                class: "text-sm font-mono font-semibold text-gray-100",
                                "{type_name}"
                            }
                            span {
                                class: "text-[10px] font-semibold {severity_text} uppercase tracking-wider",
                                "PC {pc_hex}"
                            }
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
                    div {
                        class: "mt-3 flex items-center space-x-4 text-xs text-gray-500",
                        span {
                            class: "font-mono",
                            "{fault.device_id}"
                        }
                    }
                }
            }
        }
    }
}
