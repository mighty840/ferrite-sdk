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
            class: "min-h-screen flex items-center justify-center bg-surface-950 dot-grid",
            div {
                class: "max-w-sm w-full mx-4 animate-fade-in",
                // Logo
                div {
                    class: "text-center mb-8",
                    div {
                        class: "h-14 w-14 rounded-xl bg-ferrite-600/20 border border-ferrite-600/30 flex items-center justify-center mx-auto mb-4",
                        span {
                            class: "text-ferrite-500 font-mono font-bold text-xl",
                            "Fe"
                        }
                    }
                    h1 {
                        class: "text-2xl font-semibold text-gray-100 tracking-tight",
                        "Ferrite"
                    }
                    p {
                        class: "text-sm text-gray-500 mt-1 font-mono",
                        "device observability"
                    }
                }
                div {
                    class: "bg-surface-900 rounded-xl border border-surface-700 p-6",
                    p {
                        class: "text-sm text-gray-400 mb-5 text-center",
                        "Authenticate with your identity provider"
                    }
                    button {
                        class: "w-full flex justify-center items-center py-2.5 px-4 rounded-lg text-sm font-medium text-white bg-ferrite-600 hover:bg-ferrite-500 focus:outline-none focus:ring-2 focus:ring-ferrite-500/50 transition-all duration-150",
                        onclick: move |_| {
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
                        "Sign in with SSO"
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
            class: "min-h-screen flex items-center justify-center bg-surface-950 dot-grid",
            div {
                class: "max-w-sm w-full mx-4 animate-fade-in",
                // Logo
                div {
                    class: "text-center mb-8",
                    div {
                        class: "h-14 w-14 rounded-xl bg-ferrite-600/20 border border-ferrite-600/30 flex items-center justify-center mx-auto mb-4",
                        span {
                            class: "text-ferrite-500 font-mono font-bold text-xl",
                            "Fe"
                        }
                    }
                    h1 {
                        class: "text-2xl font-semibold text-gray-100 tracking-tight",
                        "Ferrite"
                    }
                    p {
                        class: "text-sm text-gray-500 mt-1 font-mono",
                        "device observability"
                    }
                }
                div {
                    class: "bg-surface-900 rounded-xl border border-surface-700 p-6",
                    if let Some(err) = error() {
                        div {
                            class: "rounded-lg bg-red-500/10 border border-red-500/20 p-3 mb-4",
                            p {
                                class: "text-sm text-red-400",
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
                                let credentials = format!("{}:{}", user, pass);
                                let b64 = web_sys::window()
                                    .and_then(|w| w.btoa(&credentials).ok())
                                    .unwrap_or_default();

                                let token = AuthToken::Basic(b64.clone());

                                // Use same-origin; dx serve proxies to ferrite-server in dev
                                let api_url = web_sys::window()
                                    .and_then(|w| w.location().origin().ok())
                                    .unwrap_or_else(|| "http://localhost:4000".into());

                                let client = reqwest::Client::new();
                                let resp = client
                                    .get(format!("{}/devices", api_url))
                                    .header("Authorization", format!("Basic {}", b64))
                                    .send()
                                    .await;

                                loading.set(false);

                                match resp {
                                    Ok(r) if r.status().is_success() || r.status().as_u16() == 404 => {
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
                                        error.set(Some("Invalid credentials".into()));
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
                                class: "block text-xs font-medium text-gray-400 mb-1.5 uppercase tracking-wider",
                                "Username"
                            }
                            input {
                                class: "w-full px-3 py-2.5 bg-surface-800 border border-surface-650 rounded-lg text-sm text-gray-200 placeholder-gray-600 focus:ring-2 focus:ring-ferrite-500/40 focus:border-ferrite-600 outline-none transition-all font-mono",
                                r#type: "text",
                                placeholder: "admin",
                                value: "{username}",
                                oninput: move |e| username.set(e.value()),
                            }
                        }
                        div {
                            label {
                                class: "block text-xs font-medium text-gray-400 mb-1.5 uppercase tracking-wider",
                                "Password"
                            }
                            input {
                                class: "w-full px-3 py-2.5 bg-surface-800 border border-surface-650 rounded-lg text-sm text-gray-200 placeholder-gray-600 focus:ring-2 focus:ring-ferrite-500/40 focus:border-ferrite-600 outline-none transition-all font-mono",
                                r#type: "password",
                                placeholder: "password",
                                value: "{password}",
                                oninput: move |e| password.set(e.value()),
                            }
                        }
                        button {
                            class: "w-full flex justify-center py-2.5 px-4 rounded-lg text-sm font-medium text-white bg-ferrite-600 hover:bg-ferrite-500 focus:outline-none focus:ring-2 focus:ring-ferrite-500/50 transition-all duration-150 disabled:opacity-40 disabled:cursor-not-allowed",
                            r#type: "submit",
                            disabled: loading(),
                            if loading() {
                                "Authenticating..."
                            } else {
                                "Sign in"
                            }
                        }
                    }
                }
                p {
                    class: "text-center text-[10px] text-gray-600 mt-4 font-mono",
                    "Ferrite Observability Platform"
                }
            }
        }
    }
}
