use dioxus::prelude::*;

#[component]
pub fn SettingsPage() -> Element {
    let mut api_url = use_signal(|| "http://localhost:8080".to_string());
    let mut oidc_authority = use_signal(|| "https://auth.example.com".to_string());
    let mut oidc_client_id = use_signal(|| "ferrite-dashboard".to_string());
    let mut refresh_interval = use_signal(|| "5".to_string());
    let mut dark_mode = use_signal(|| true);
    let mut notifications = use_signal(|| true);
    let mut saved = use_signal(|| false);

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

            // API Configuration
            div {
                class: "bg-surface-900 rounded-xl border border-surface-700 mb-6",
                div {
                    class: "px-5 py-4 border-b border-surface-700",
                    h2 {
                        class: "text-sm font-medium text-gray-200",
                        "API Configuration"
                    }
                    p {
                        class: "text-xs text-gray-500 mt-0.5",
                        "Connection to the ferrite backend"
                    }
                }
                div {
                    class: "px-5 py-4 space-y-4",
                    SettingsField {
                        label: "API Base URL",
                        help: "The base URL of the ferrite REST API server",
                        value: api_url(),
                        on_change: move |e: Event<FormData>| api_url.set(e.value()),
                    }
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
                }
            }

            // Authentication
            div {
                class: "bg-surface-900 rounded-xl border border-surface-700 mb-6",
                div {
                    class: "px-5 py-4 border-b border-surface-700",
                    h2 {
                        class: "text-sm font-medium text-gray-200",
                        "Authentication"
                    }
                    p {
                        class: "text-xs text-gray-500 mt-0.5",
                        "OpenID Connect settings"
                    }
                }
                div {
                    class: "px-5 py-4 space-y-4",
                    SettingsField {
                        label: "OIDC Authority",
                        help: "The OIDC provider authority URL",
                        value: oidc_authority(),
                        on_change: move |e: Event<FormData>| oidc_authority.set(e.value()),
                    }
                    SettingsField {
                        label: "Client ID",
                        help: "The OIDC client identifier",
                        value: oidc_client_id(),
                        on_change: move |e: Event<FormData>| oidc_client_id.set(e.value()),
                    }
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
                        saved.set(true);
                    },
                    "Save Settings"
                }
            }
        }
    }
}

#[component]
fn SettingsField(
    label: &'static str,
    help: &'static str,
    value: String,
    on_change: EventHandler<Event<FormData>>,
) -> Element {
    rsx! {
        div {
            label {
                class: "block text-xs font-medium text-gray-400 mb-1.5 uppercase tracking-wider",
                "{label}"
            }
            input {
                class: "w-full px-3 py-2.5 bg-surface-800 border border-surface-650 rounded-lg text-sm text-gray-200 placeholder-gray-600 focus:ring-2 focus:ring-ferrite-500/40 focus:border-ferrite-600 outline-none transition-all font-mono",
                r#type: "text",
                value: "{value}",
                oninput: move |e| on_change.call(e),
            }
            p {
                class: "mt-1 text-[10px] text-gray-600 font-mono",
                "{help}"
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
