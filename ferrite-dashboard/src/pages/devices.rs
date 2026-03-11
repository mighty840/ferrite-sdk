use crate::api::types::*;
use crate::auth::AuthState;
use crate::components::{DeviceCard, ErrorDisplay, Loading};
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn DevicesPage() -> Element {
    let mut search = use_signal(|| String::new());
    let mut status_filter = use_signal(|| "all".to_string());
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

    let binding = devices_resource.read();
    match &*binding {
        None => rsx! { Loading {} },
        Some(Err(e)) => rsx! { ErrorDisplay { message: e.to_string() } },
        Some(Ok(devices)) => {
            let filtered: Vec<&Device> = devices
                .iter()
                .filter(|d| {
                    let matches_search = search().is_empty()
                        || d.display_name()
                            .to_lowercase()
                            .contains(&search().to_lowercase())
                        || d.device_id
                            .to_lowercase()
                            .contains(&search().to_lowercase())
                        || d.key_display()
                            .to_lowercase()
                            .contains(&search().to_lowercase());
                    let matches_status = match status_filter().as_str() {
                        "online" => d.status_str() == "online",
                        "offline" => d.status_str() == "offline",
                        "degraded" => d.status_str() == "degraded",
                        "provisioned" => d.status_str() == "provisioned",
                        _ => true,
                    };
                    matches_search && matches_status
                })
                .collect();

            let count = filtered.len();

            rsx! {
                div {
                    class: "p-6 lg:p-8 max-w-[1400px] mx-auto",
                    div {
                        class: "mb-6 animate-fade-in",
                        div {
                            class: "flex items-center justify-between",
                            div {
                                h1 {
                                    class: "text-2xl font-semibold text-gray-100",
                                    "Devices"
                                }
                                p {
                                    class: "mt-1 text-sm text-gray-500",
                                    "Manage and monitor your device fleet"
                                }
                            }
                            Link {
                                to: Route::Register {},
                                class: "inline-flex items-center px-4 py-2 bg-ferrite-600 text-white rounded-lg hover:bg-ferrite-500 text-sm font-medium transition-colors",
                                "Register Device"
                            }
                        }
                    }

                    // Filters
                    div {
                        class: "flex flex-col sm:flex-row gap-3 mb-6",
                        div {
                            class: "flex-1",
                            div {
                                class: "relative",
                                svg {
                                    class: "absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-gray-600",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                                    }
                                }
                                input {
                                    class: "w-full pl-10 pr-4 py-2.5 bg-surface-900 border border-surface-700 rounded-lg text-sm text-gray-200 placeholder-gray-600 focus:ring-2 focus:ring-ferrite-500/40 focus:border-ferrite-600 outline-none transition-all font-mono",
                                    r#type: "text",
                                    placeholder: "Search devices...",
                                    value: "{search}",
                                    oninput: move |e| search.set(e.value()),
                                }
                            }
                        }
                        select {
                            class: "px-4 py-2.5 bg-surface-900 border border-surface-700 rounded-lg text-sm text-gray-300 focus:ring-2 focus:ring-ferrite-500/40 focus:border-ferrite-600 outline-none transition-all",
                            value: "{status_filter}",
                            onchange: move |e| status_filter.set(e.value()),
                            option { value: "all", "All statuses" }
                            option { value: "online", "Online" }
                            option { value: "provisioned", "Provisioned" }
                            option { value: "degraded", "Degraded" }
                            option { value: "offline", "Offline" }
                        }
                    }

                    p {
                        class: "text-[10px] text-gray-600 mb-4 font-mono uppercase tracking-wider",
                        "{count} device(s)"
                    }

                    if devices.is_empty() {
                        div {
                            class: "bg-surface-900 rounded-xl border border-surface-700 p-12 text-center",
                            p {
                                class: "text-gray-500 text-sm mb-4",
                                "No devices registered yet"
                            }
                            Link {
                                to: Route::Register {},
                                class: "inline-flex items-center px-4 py-2 bg-ferrite-600 text-white rounded-lg hover:bg-ferrite-500 text-sm font-medium transition-colors",
                                "Register your first device"
                            }
                        }
                    } else {
                        div {
                            class: "grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-4",
                            for device in filtered {
                                DeviceCard { device: device.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}
