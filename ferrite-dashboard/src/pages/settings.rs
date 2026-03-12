use crate::auth::AuthState;
use dioxus::prelude::*;

fn get_local_storage() -> Option<web_sys::Storage> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
}

fn load_setting(key: &str, default: &str) -> String {
    get_local_storage()
        .and_then(|s| s.get_item(key).ok())
        .flatten()
        .unwrap_or_else(|| default.to_string())
}

fn save_setting(key: &str, value: &str) {
    if let Some(storage) = get_local_storage() {
        let _ = storage.set_item(key, value);
    }
}

#[component]
pub fn SettingsPage() -> Element {
    let auth_state = use_context::<Signal<AuthState>>();

    let mut refresh_interval = use_signal(|| load_setting("ferrite_refresh_interval", "5"));
    let mut dark_mode = use_signal(|| load_setting("ferrite_dark_mode", "true") == "true");
    let mut notifications = use_signal(|| load_setting("ferrite_notifications", "true") == "true");
    let mut saved = use_signal(|| false);

    // Derive server info from auth state
    let (auth_mode_label, auth_detail) = match auth_state() {
        AuthState::Authenticated { .. } | AuthState::Loading => ("Detected".into(), "—".into()),
        AuthState::Unauthenticated { ref auth_mode } => {
            let mode = auth_mode.mode.clone();
            let detail = match mode.as_str() {
                "keycloak" => auth_mode.authority.clone().unwrap_or_else(|| "—".into()),
                _ => "Built-in credentials".into(),
            };
            (mode, detail)
        }
    };

    let api_url = crate::api::client::api_url();

    rsx! {
        div {
            class: "p-6 lg:p-8 max-w-3xl mx-auto",
            div {
                class: "mb-6 animate-fade-in",
                h1 {
                    class: "text-2xl font-semibold text-gray-100",
                    "Settings"
                }
                p {
                    class: "mt-1 text-sm text-gray-500",
                    "Configure dashboard and connection settings"
                }
            }

            if saved() {
                div {
                    class: "mb-6 rounded-lg bg-emerald-500/10 border border-emerald-500/20 p-3",
                    p {
                        class: "text-sm text-emerald-400 font-mono",
                        "Settings saved"
                    }
                }
            }

            // Server Info (read-only)
            div {
                class: "bg-surface-900 rounded-xl border border-surface-700 mb-6",
                div {
                    class: "px-5 py-4 border-b border-surface-700",
                    h2 {
                        class: "text-sm font-medium text-gray-200",
                        "Server"
                    }
                    p {
                        class: "text-xs text-gray-500 mt-0.5",
                        "Discovered from the ferrite backend"
                    }
                }
                div {
                    class: "px-5 py-4 space-y-3",
                    ReadOnlyField { label: "API URL", value: api_url }
                    ReadOnlyField { label: "Auth Mode", value: auth_mode_label }
                    ReadOnlyField { label: "Auth Detail", value: auth_detail }
                }
            }

            // Preferences
            div {
                class: "bg-surface-900 rounded-xl border border-surface-700 mb-6",
                div {
                    class: "px-5 py-4 border-b border-surface-700",
                    h2 {
                        class: "text-sm font-medium text-gray-200",
                        "Preferences"
                    }
                }
                div {
                    class: "px-5 py-4 space-y-4",
                    div {
                        label {
                            class: "block text-xs font-medium text-gray-400 mb-1.5 uppercase tracking-wider",
                            "Refresh Interval"
                        }
                        select {
                            class: "w-full px-3 py-2.5 bg-surface-800 border border-surface-650 rounded-lg text-sm text-gray-200 focus:ring-2 focus:ring-ferrite-500/40 focus:border-ferrite-600 outline-none transition-all",
                            value: "{refresh_interval}",
                            onchange: move |e| refresh_interval.set(e.value()),
                            option { value: "1", "1 second" }
                            option { value: "5", "5 seconds" }
                            option { value: "10", "10 seconds" }
                            option { value: "30", "30 seconds" }
                            option { value: "60", "60 seconds" }
                        }
                        p {
                            class: "mt-1 text-[10px] text-gray-600 font-mono",
                            "Polling interval for data refresh"
                        }
                    }
                    ToggleSetting {
                        label: "Dark Mode",
                        description: "Use dark theme for the dashboard",
                        enabled: dark_mode(),
                        on_toggle: move |_| dark_mode.set(!dark_mode()),
                    }
                    ToggleSetting {
                        label: "Notifications",
                        description: "Browser notifications for critical faults",
                        enabled: notifications(),
                        on_toggle: move |_| notifications.set(!notifications()),
                    }
                }
            }

            div {
                class: "flex justify-end",
                button {
                    class: "px-6 py-2.5 bg-ferrite-600 text-white rounded-lg hover:bg-ferrite-500 focus:ring-2 focus:ring-ferrite-500/50 text-sm font-medium transition-all duration-150",
                    onclick: move |_| {
                        save_setting("ferrite_refresh_interval", &refresh_interval());
                        save_setting("ferrite_dark_mode", if dark_mode() { "true" } else { "false" });
                        save_setting("ferrite_notifications", if notifications() { "true" } else { "false" });
                        saved.set(true);
                    },
                    "Save Settings"
                }
            }
        }
    }
}

#[component]
fn ReadOnlyField(label: &'static str, value: String) -> Element {
    rsx! {
        div {
            class: "flex items-center justify-between",
            span {
                class: "text-xs font-medium text-gray-500 uppercase tracking-wider",
                "{label}"
            }
            span {
                class: "text-sm text-gray-300 font-mono",
                "{value}"
            }
        }
    }
}

#[component]
fn ToggleSetting(
    label: &'static str,
    description: &'static str,
    enabled: bool,
    on_toggle: EventHandler<MouseEvent>,
) -> Element {
    let toggle_bg = if enabled {
        "bg-ferrite-600"
    } else {
        "bg-surface-650"
    };
    let toggle_pos = if enabled {
        "translate-x-5"
    } else {
        "translate-x-0"
    };

    rsx! {
        div {
            class: "flex items-center justify-between py-2",
            div {
                p {
                    class: "text-sm font-medium text-gray-200",
                    "{label}"
                }
                p {
                    class: "text-xs text-gray-500",
                    "{description}"
                }
            }
            button {
                class: "relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out {toggle_bg}",
                onclick: move |e| on_toggle.call(e),
                span {
                    class: "inline-block h-5 w-5 transform rounded-full bg-white shadow transition duration-200 ease-in-out {toggle_pos}"
                }
            }
        }
    }
}
