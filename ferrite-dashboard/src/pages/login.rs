use dioxus::prelude::*;

use crate::auth::{AuthModeInfo, AuthState, AuthToken, UserInfo};

/// Login page that adapts to the server's auth mode.
#[component]
pub fn LoginPage(auth_state: Signal<AuthState>, auth_mode: AuthModeInfo) -> Element {
    match auth_mode.mode.as_str() {
        "keycloak" => rsx! { KeycloakLogin { auth_mode } },
        _ => rsx! { BasicLogin { auth_state } },
    }
}

/// Keycloak OIDC login — redirects to Keycloak authorization endpoint.
#[component]
fn KeycloakLogin(auth_mode: AuthModeInfo) -> Element {
    let authority = auth_mode.authority.clone().unwrap_or_default();
    let client_id = auth_mode.client_id.clone().unwrap_or_default();

    rsx! {
        div {
            class: "min-h-screen flex items-center justify-center bg-gray-50",
            div {
                class: "max-w-md w-full space-y-8 p-8",
                div {
                    class: "text-center",
                    h1 {
                        class: "text-3xl font-bold text-ferrite-900",
                        "ferrite"
                    }
                    p {
                        class: "mt-2 text-sm text-gray-500",
                        "Sign in to the dashboard"
                    }
                }
                div {
                    class: "bg-white rounded-lg shadow border border-gray-200 p-6",
                    p {
                        class: "text-sm text-gray-600 mb-4 text-center",
                        "Authenticate with your organization's identity provider."
                    }
                    button {
                        class: "w-full flex justify-center py-2 px-4 border border-transparent rounded-lg shadow-sm text-sm font-medium text-white bg-ferrite-600 hover:bg-ferrite-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-ferrite-500 transition-colors duration-150",
                        onclick: move |_| {
                            // Redirect to Keycloak authorization endpoint
                            let auth_url = format!(
                                "{}/protocol/openid-connect/auth?response_type=code&client_id={}&redirect_uri={}&scope=openid%20profile%20email",
                                authority,
                                client_id,
                                web_sys::window()
                                    .and_then(|w| w.location().origin().ok())
                                    .unwrap_or_default()
                                    + "/callback"
                            );
                            if let Some(window) = web_sys::window() {
                                let _ = window.location().set_href(&auth_url);
                            }
                        },
                        "Sign in with Keycloak"
                    }
                }
            }
        }
    }
}

/// Basic auth login form.
#[component]
fn BasicLogin(auth_state: Signal<AuthState>) -> Element {
    let mut username = use_signal(|| String::new());
    let mut password = use_signal(|| String::new());
    let mut error = use_signal(|| Option::<String>::None);
    let mut loading = use_signal(|| false);
    let mut auth_state = auth_state;

    rsx! {
        div {
            class: "min-h-screen flex items-center justify-center bg-gray-50",
            div {
                class: "max-w-md w-full space-y-8 p-8",
                div {
                    class: "text-center",
                    h1 {
                        class: "text-3xl font-bold text-ferrite-900",
                        "ferrite"
                    }
                    p {
                        class: "mt-2 text-sm text-gray-500",
                        "Sign in to the dashboard"
                    }
                }
                div {
                    class: "bg-white rounded-lg shadow border border-gray-200 p-6",
                    if let Some(err) = error() {
                        div {
                            class: "rounded-lg bg-red-50 border border-red-200 p-3 mb-4",
                            p {
                                class: "text-sm text-red-700",
                                "{err}"
                            }
                        }
                    }
                    form {
                        class: "space-y-4",
                        onsubmit: move |e| {
                            e.prevent_default();
                            let user = username();
                            let pass = password();
                            loading.set(true);
                            error.set(None);
                            spawn(async move {
                                // Encode credentials as base64 for Basic auth
                                let credentials = format!("{}:{}", user, pass);
                                // Use btoa for base64
                                let b64 = web_sys::window()
                                    .and_then(|w| w.btoa(&credentials).ok())
                                    .unwrap_or_default();

                                let token = AuthToken::Basic(b64.clone());

                                // Test the credentials against the server
                                let api_url = web_sys::window()
                                    .and_then(|w| w.location().origin().ok())
                                    .unwrap_or_else(|| "http://localhost:4000".into());

                                let client = reqwest::Client::new();
                                let resp = client
                                    .get(format!("{}/devices", api_url.replace(":8080", ":4000")))
                                    .header("Authorization", format!("Basic {}", b64))
                                    .send()
                                    .await;

                                loading.set(false);

                                match resp {
                                    Ok(r) if r.status().is_success() || r.status().as_u16() == 404 => {
                                        // Store in session storage
                                        if let Some(storage) = web_sys::window()
                                            .and_then(|w| w.session_storage().ok())
                                            .flatten()
                                        {
                                            let _ = storage.set_item("ferrite_auth_token", &b64);
                                            let _ = storage.set_item("ferrite_auth_type", "basic");
                                            let _ = storage.set_item("ferrite_auth_user", &user);
                                        }
                                        auth_state.set(AuthState::Authenticated {
                                            user: UserInfo {
                                                name: user,
                                                email: None,
                                            },
                                            token,
                                        });
                                    }
                                    Ok(r) if r.status().as_u16() == 401 => {
                                        error.set(Some("Invalid username or password".into()));
                                    }
                                    Ok(r) => {
                                        error.set(Some(format!("Server error: HTTP {}", r.status())));
                                    }
                                    Err(e) => {
                                        error.set(Some(format!("Connection failed: {}", e)));
                                    }
                                }
                            });
                        },
                        div {
                            label {
                                class: "block text-sm font-medium text-gray-700 mb-1",
                                "Username"
                            }
                            input {
                                class: "w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-ferrite-500 focus:border-ferrite-500 outline-none",
                                r#type: "text",
                                placeholder: "admin",
                                value: "{username}",
                                oninput: move |e| username.set(e.value()),
                            }
                        }
                        div {
                            label {
                                class: "block text-sm font-medium text-gray-700 mb-1",
                                "Password"
                            }
                            input {
                                class: "w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-ferrite-500 focus:border-ferrite-500 outline-none",
                                r#type: "password",
                                placeholder: "password",
                                value: "{password}",
                                oninput: move |e| password.set(e.value()),
                            }
                        }
                        button {
                            class: "w-full flex justify-center py-2 px-4 border border-transparent rounded-lg shadow-sm text-sm font-medium text-white bg-ferrite-600 hover:bg-ferrite-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-ferrite-500 transition-colors duration-150 disabled:opacity-50",
                            r#type: "submit",
                            disabled: loading(),
                            if loading() {
                                "Signing in..."
                            } else {
                                "Sign in"
                            }
                        }
                    }
                }
            }
        }
    }
}
