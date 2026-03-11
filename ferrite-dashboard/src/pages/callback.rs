use dioxus::prelude::*;

use crate::auth::{AuthState, AuthToken, OidcClient, UserInfo};
use crate::Route;

/// OIDC callback page — exchanges authorization code for token after Keycloak redirect.
#[component]
pub fn CallbackPage(code: String, state: String) -> Element {
    let mut auth_state = use_context::<Signal<AuthState>>();
    let mut error = use_signal(|| Option::<String>::None);

    // Exchange the authorization code on mount
    use_effect(move || {
        let code = code.clone();
        spawn(async move {
            // Retrieve OIDC config from session storage
            let storage = web_sys::window()
                .and_then(|w| w.session_storage().ok())
                .flatten();

            let (client_id, authority) = match &storage {
                Some(s) => {
                    let cid = s
                        .get_item("ferrite_oidc_client_id")
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    let auth = s
                        .get_item("ferrite_oidc_authority")
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    (cid, auth)
                }
                None => {
                    error.set(Some("Session storage unavailable".into()));
                    return;
                }
            };

            if client_id.is_empty() || authority.is_empty() {
                error.set(Some(
                    "Missing OIDC configuration. Please start login again.".into(),
                ));
                return;
            }

            let redirect_uri = web_sys::window()
                .and_then(|w| w.location().origin().ok())
                .unwrap_or_default()
                + "/callback";

            let oidc = OidcClient::new(&client_id, &authority, &redirect_uri);

            match oidc.handle_callback(&code).await {
                Ok(token_set) => {
                    // Store session
                    if let Some(s) = &storage {
                        let _ = s.set_item("ferrite_auth_token", &token_set.access_token);
                        let _ = s.set_item("ferrite_auth_type", "bearer");
                        let _ = s.set_item("ferrite_auth_user", "SSO User");
                        // Clean up PKCE verifier and OIDC config
                        let _ = s.remove_item("ferrite_pkce_verifier");
                        let _ = s.remove_item("ferrite_oidc_client_id");
                        let _ = s.remove_item("ferrite_oidc_authority");
                    }

                    auth_state.set(AuthState::Authenticated {
                        user: UserInfo {
                            name: "SSO User".into(),
                            email: None,
                        },
                        token: AuthToken::Bearer(token_set.access_token),
                    });

                    // Navigate to dashboard
                    navigator().push(Route::Dashboard {});
                }
                Err(e) => {
                    error.set(Some(format!("Authentication failed: {}", e)));
                }
            }
        });
    });

    if let Some(err) = error() {
        rsx! {
            div {
                class: "min-h-screen flex items-center justify-center bg-surface-950 dot-grid",
                div {
                    class: "max-w-sm w-full mx-4 animate-fade-in",
                    div {
                        class: "bg-surface-900 rounded-xl border border-surface-700 p-6 text-center",
                        div {
                            class: "rounded-lg bg-red-500/10 border border-red-500/20 p-3 mb-4",
                            p {
                                class: "text-sm text-red-400",
                                "{err}"
                            }
                        }
                        Link {
                            to: Route::Dashboard {},
                            class: "inline-flex items-center px-4 py-2 bg-ferrite-600 text-white rounded-lg hover:bg-ferrite-500 text-sm font-medium transition-colors duration-150",
                            "Back to Login"
                        }
                    }
                }
            }
        }
    } else {
        rsx! {
            div {
                class: "min-h-screen flex items-center justify-center bg-surface-950",
                div {
                    class: "text-center animate-fade-in",
                    div {
                        class: "h-12 w-12 rounded-xl bg-ferrite-600/20 border border-ferrite-600/30 flex items-center justify-center mx-auto mb-4",
                        span {
                            class: "text-ferrite-500 font-mono font-bold text-lg",
                            "Fe"
                        }
                    }
                    div {
                        class: "animate-spin h-6 w-6 border-2 border-ferrite-500 border-t-transparent rounded-full mx-auto mb-4"
                    }
                    p { class: "text-gray-500 text-sm font-mono", "completing authentication..." }
                }
            }
        }
    }
}
