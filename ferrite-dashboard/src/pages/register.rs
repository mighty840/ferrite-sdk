use crate::auth::AuthState;
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn RegisterPage() -> Element {
    let auth_state = use_context::<Signal<AuthState>>();
    let mut key_input = use_signal(|| String::new());
    let mut name_input = use_signal(|| String::new());
    let mut tags_input = use_signal(|| String::new());
    let mut status_msg = use_signal(|| Option::<(bool, String)>::None);
    let mut submitting = use_signal(|| false);

    let key_valid = {
        let k = key_input();
        let clean: String = k.replace('-', "").replace("0x", "").replace("0X", "");
        clean.len() == 8 && clean.chars().all(|c| c.is_ascii_hexdigit())
    };

    let on_submit = move |_| {
        let key = key_input().clone();
        let name = name_input().clone();
        let tags = tags_input().clone();
        let auth = auth_state();
        submitting.set(true);
        status_msg.set(None);

        spawn(async move {
            let client = crate::api::client::authenticated_client(&auth);
            let name_opt = if name.is_empty() { None } else { Some(name) };
            let tags_opt = if tags.is_empty() { None } else { Some(tags) };

            match client.register_device(&key, name_opt, tags_opt).await {
                Ok(dev) => {
                    status_msg.set(Some((
                        true,
                        format!("Registered device: {}", dev.display_name()),
                    )));
                    key_input.set(String::new());
                    name_input.set(String::new());
                    tags_input.set(String::new());
                }
                Err(e) => {
                    status_msg.set(Some((false, format!("Error: {e}"))));
                }
            }
            submitting.set(false);
        });
    };

    rsx! {
        div {
            class: "p-6 lg:p-8 max-w-[800px] mx-auto",
            div {
                class: "mb-6 animate-fade-in",
                div {
                    class: "flex items-center space-x-3 mb-2",
                    Link {
                        to: Route::Devices {},
                        class: "text-xs text-gray-500 hover:text-gray-300 transition-colors",
                        "Devices"
                    }
                    span { class: "text-gray-600 text-xs", "/" }
                    span { class: "text-gray-400 text-xs", "Register" }
                }
                h1 {
                    class: "text-2xl font-semibold text-gray-100",
                    "Register Device"
                }
                p {
                    class: "mt-1 text-sm text-gray-500",
                    "Register a provisioned device by its hex key"
                }
            }

            // Status message
            if let Some((success, msg)) = status_msg() {
                div {
                    class: if success {
                        "mb-6 p-4 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-sm text-emerald-400"
                    } else {
                        "mb-6 p-4 rounded-lg bg-red-500/10 border border-red-500/20 text-sm text-red-400"
                    },
                    "{msg}"
                }
            }

            // Registration form
            div {
                class: "bg-surface-900 rounded-xl border border-surface-700 p-6",
                div {
                    class: "space-y-5",
                    // Device Key
                    div {
                        label {
                            class: "block text-xs font-medium text-gray-400 uppercase tracking-wider mb-2",
                            "Device Key (hex)"
                        }
                        input {
                            class: "w-full px-4 py-2.5 bg-surface-800 border border-surface-600 rounded-lg text-sm text-gray-200 placeholder-gray-600 focus:ring-2 focus:ring-ferrite-500/40 focus:border-ferrite-600 outline-none transition-all font-mono uppercase",
                            r#type: "text",
                            placeholder: "A3-00F1B2 or A300F1B2",
                            maxlength: "10",
                            value: "{key_input}",
                            oninput: move |e| key_input.set(e.value()),
                        }
                        if !key_input().is_empty() && !key_valid {
                            p {
                                class: "mt-1 text-xs text-red-400",
                                "Must be 8 hex characters (e.g. A300F1B2)"
                            }
                        }
                    }
                    // Name
                    div {
                        label {
                            class: "block text-xs font-medium text-gray-400 uppercase tracking-wider mb-2",
                            "Name (optional)"
                        }
                        input {
                            class: "w-full px-4 py-2.5 bg-surface-800 border border-surface-600 rounded-lg text-sm text-gray-200 placeholder-gray-600 focus:ring-2 focus:ring-ferrite-500/40 focus:border-ferrite-600 outline-none transition-all",
                            r#type: "text",
                            placeholder: "e.g. Temperature Sensor A",
                            value: "{name_input}",
                            oninput: move |e| name_input.set(e.value()),
                        }
                    }
                    // Tags
                    div {
                        label {
                            class: "block text-xs font-medium text-gray-400 uppercase tracking-wider mb-2",
                            "Tags (optional, comma-separated)"
                        }
                        input {
                            class: "w-full px-4 py-2.5 bg-surface-800 border border-surface-600 rounded-lg text-sm text-gray-200 placeholder-gray-600 focus:ring-2 focus:ring-ferrite-500/40 focus:border-ferrite-600 outline-none transition-all font-mono",
                            r#type: "text",
                            placeholder: "production, floor-1",
                            value: "{tags_input}",
                            oninput: move |e| tags_input.set(e.value()),
                        }
                    }
                    // Submit
                    button {
                        class: "w-full px-4 py-3 bg-ferrite-600 text-white rounded-lg hover:bg-ferrite-500 text-sm font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: !key_valid || submitting(),
                        onclick: on_submit,
                        if submitting() { "Registering..." } else { "Register Device" }
                    }
                }
            }
        }
    }
}
