use dioxus::prelude::*;

#[component]
pub fn SettingsPage() -> Element {
    let mut api_url = use_signal(|| "http://localhost:8080".to_string());
    let mut oidc_authority = use_signal(|| "https://auth.example.com".to_string());
    let mut oidc_client_id = use_signal(|| "iotai-dashboard".to_string());
    let mut refresh_interval = use_signal(|| "5".to_string());
    let mut dark_mode = use_signal(|| false);
    let mut notifications = use_signal(|| true);
    let mut saved = use_signal(|| false);

    rsx! {
        div {
            class: "max-w-3xl mx-auto px-4 sm:px-6 lg:px-8 py-8",
            div {
                class: "mb-6",
                h1 {
                    class: "text-2xl font-bold text-gray-900",
                    "Settings"
                }
                p {
                    class: "mt-1 text-sm text-gray-500",
                    "Configure dashboard and connection settings"
                }
            }

            if saved() {
                div {
                    class: "mb-6 rounded-lg bg-green-50 border border-green-200 p-4",
                    div {
                        class: "flex items-center",
                        svg {
                            class: "h-5 w-5 text-green-500 mr-3",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M5 13l4 4L19 7"
                            }
                        }
                        p {
                            class: "text-sm text-green-700",
                            "Settings saved successfully"
                        }
                    }
                }
            }

            // API Configuration
            div {
                class: "bg-white rounded-lg shadow border border-gray-200 mb-6",
                div {
                    class: "px-6 py-4 border-b border-gray-200",
                    h2 {
                        class: "text-lg font-semibold text-gray-900",
                        "API Configuration"
                    }
                    p {
                        class: "text-sm text-gray-500",
                        "Configure the connection to the iotai backend"
                    }
                }
                div {
                    class: "px-6 py-4 space-y-4",
                    SettingsField {
                        label: "API Base URL",
                        help: "The base URL of the iotai REST API server",
                        value: api_url(),
                        on_change: move |e: Event<FormData>| api_url.set(e.value()),
                    }
                    div {
                        label {
                            class: "block text-sm font-medium text-gray-700 mb-1",
                            "Refresh Interval (seconds)"
                        }
                        select {
                            class: "w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-iotai-500 focus:border-iotai-500 outline-none bg-white",
                            value: "{refresh_interval}",
                            onchange: move |e| refresh_interval.set(e.value()),
                            option { value: "1", "1 second" }
                            option { value: "5", "5 seconds" }
                            option { value: "10", "10 seconds" }
                            option { value: "30", "30 seconds" }
                            option { value: "60", "60 seconds" }
                        }
                        p {
                            class: "mt-1 text-xs text-gray-500",
                            "How often to poll for new data"
                        }
                    }
                }
            }

            // Authentication
            div {
                class: "bg-white rounded-lg shadow border border-gray-200 mb-6",
                div {
                    class: "px-6 py-4 border-b border-gray-200",
                    h2 {
                        class: "text-lg font-semibold text-gray-900",
                        "Authentication"
                    }
                    p {
                        class: "text-sm text-gray-500",
                        "OpenID Connect settings for secure authentication"
                    }
                }
                div {
                    class: "px-6 py-4 space-y-4",
                    SettingsField {
                        label: "OIDC Authority",
                        help: "The OIDC provider authority URL",
                        value: oidc_authority(),
                        on_change: move |e: Event<FormData>| oidc_authority.set(e.value()),
                    }
                    SettingsField {
                        label: "Client ID",
                        help: "The OIDC client identifier for this dashboard",
                        value: oidc_client_id(),
                        on_change: move |e: Event<FormData>| oidc_client_id.set(e.value()),
                    }
                }
            }

            // Preferences
            div {
                class: "bg-white rounded-lg shadow border border-gray-200 mb-6",
                div {
                    class: "px-6 py-4 border-b border-gray-200",
                    h2 {
                        class: "text-lg font-semibold text-gray-900",
                        "Preferences"
                    }
                }
                div {
                    class: "px-6 py-4 space-y-4",
                    ToggleSetting {
                        label: "Dark Mode",
                        description: "Use dark theme for the dashboard",
                        enabled: dark_mode(),
                        on_toggle: move |_| dark_mode.set(!dark_mode()),
                    }
                    ToggleSetting {
                        label: "Notifications",
                        description: "Show browser notifications for critical faults",
                        enabled: notifications(),
                        on_toggle: move |_| notifications.set(!notifications()),
                    }
                }
            }

            // Save button
            div {
                class: "flex justify-end",
                button {
                    class: "px-6 py-2 bg-iotai-600 text-white rounded-lg hover:bg-iotai-700 focus:ring-2 focus:ring-offset-2 focus:ring-iotai-500 text-sm font-medium transition-colors duration-150",
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
                class: "block text-sm font-medium text-gray-700 mb-1",
                "{label}"
            }
            input {
                class: "w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-iotai-500 focus:border-iotai-500 outline-none",
                r#type: "text",
                value: "{value}",
                oninput: move |e| on_change.call(e),
            }
            p {
                class: "mt-1 text-xs text-gray-500",
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
    let toggle_bg = if enabled { "bg-iotai-600" } else { "bg-gray-200" };
    let toggle_pos = if enabled { "translate-x-5" } else { "translate-x-0" };

    rsx! {
        div {
            class: "flex items-center justify-between py-2",
            div {
                p {
                    class: "text-sm font-medium text-gray-700",
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
