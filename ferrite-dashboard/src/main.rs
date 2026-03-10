pub mod api;
pub mod auth;
pub mod components;
pub mod pages;

use dioxus::prelude::*;

use auth::{AuthModeInfo, AuthState, AuthToken, UserInfo};
use components::Navbar;
use pages::*;

/// Route definitions for the ferrite dashboard.
#[derive(Clone, Routable, Debug, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(AppLayout)]
        #[route("/")]
        Dashboard {},
        #[route("/devices")]
        Devices {},
        #[route("/devices/:id")]
        DeviceDetail { id: String },
        #[route("/faults")]
        Faults {},
        #[route("/metrics")]
        Metrics {},
        #[route("/settings")]
        Settings {},
    #[end_layout]
    #[route("/:..route")]
    NotFound { route: Vec<String> },
}

/// Layout wrapper with auth guard — shows login if unauthenticated.
#[component]
fn AppLayout() -> Element {
    let auth_state = use_context::<Signal<AuthState>>();

    match auth_state() {
        AuthState::Loading => rsx! {
            div {
                class: "min-h-screen flex items-center justify-center",
                div {
                    class: "text-center",
                    div {
                        class: "animate-spin h-8 w-8 border-4 border-ferrite-500 border-t-transparent rounded-full mx-auto mb-4"
                    }
                    p { class: "text-gray-500 text-sm", "Loading..." }
                }
            }
        },
        AuthState::Unauthenticated { auth_mode } => rsx! {
            LoginPage {
                auth_state,
                auth_mode,
            }
        },
        AuthState::Authenticated { .. } => rsx! {
            Navbar {}
            Outlet::<Route> {}
        },
    }
}

/// Dashboard home page route handler.
#[component]
fn Dashboard() -> Element {
    rsx! { DashboardPage {} }
}

/// Devices list page route handler.
#[component]
fn Devices() -> Element {
    rsx! { DevicesPage {} }
}

/// Device detail page route handler.
#[component]
fn DeviceDetail(id: String) -> Element {
    rsx! { DeviceDetailPage { id: id } }
}

/// Faults list page route handler.
#[component]
fn Faults() -> Element {
    rsx! { FaultsPage {} }
}

/// Metrics overview page route handler.
#[component]
fn Metrics() -> Element {
    rsx! { MetricsPage {} }
}

/// Settings page route handler.
#[component]
fn Settings() -> Element {
    rsx! { SettingsPage {} }
}

/// 404 not found page.
#[component]
fn NotFound(route: Vec<String>) -> Element {
    let path = route.join("/");
    rsx! {
        div {
            class: "min-h-screen flex items-center justify-center",
            div {
                class: "text-center",
                h1 {
                    class: "text-6xl font-bold text-gray-300 mb-4",
                    "404"
                }
                p {
                    class: "text-xl text-gray-600 mb-6",
                    "Page not found"
                }
                p {
                    class: "text-sm text-gray-400 mb-8",
                    "The path /{path} does not exist."
                }
                Link {
                    to: Route::Dashboard {},
                    class: "inline-flex items-center px-4 py-2 bg-ferrite-600 text-white rounded-lg hover:bg-ferrite-700 text-sm font-medium transition-colors duration-150",
                    "Go to Dashboard"
                }
            }
        }
    }
}

fn main() {
    dioxus::launch(App);
}

/// Root application component — initializes auth state.
#[component]
fn App() -> Element {
    let auth_state = use_signal(|| AuthState::Loading);
    use_context_provider(|| auth_state);

    // On mount, check for stored session and discover auth mode
    let mut auth = auth_state;
    use_effect(move || {
        spawn(async move {
            // Check session storage for existing auth
            let stored = web_sys::window()
                .and_then(|w| w.session_storage().ok())
                .flatten()
                .and_then(|s| {
                    let auth_type = s.get_item("ferrite_auth_type").ok()??;
                    let token = s.get_item("ferrite_auth_token").ok()??;
                    let user = s.get_item("ferrite_auth_user").ok()??;
                    Some((auth_type, token, user))
                });

            if let Some((auth_type, token, user)) = stored {
                let auth_token = match auth_type.as_str() {
                    "basic" => AuthToken::Basic(token),
                    _ => AuthToken::Bearer(token),
                };
                auth.set(AuthState::Authenticated {
                    user: UserInfo {
                        name: user,
                        email: None,
                    },
                    token: auth_token,
                });
                return;
            }

            // No stored session — discover auth mode from server
            // Use same-origin; dx serve proxies API requests to ferrite-server in dev
            let api_url = web_sys::window()
                .and_then(|w| w.location().origin().ok())
                .unwrap_or_else(|| "http://localhost:4000".into());

            let client = api::ApiClient::new(&api_url);
            match client.get_auth_mode().await {
                Ok(mode_info) => {
                    auth.set(AuthState::Unauthenticated {
                        auth_mode: mode_info,
                    });
                }
                Err(_) => {
                    // Fallback: assume basic auth if server unreachable
                    auth.set(AuthState::Unauthenticated {
                        auth_mode: AuthModeInfo {
                            mode: "basic".into(),
                            authority: None,
                            client_id: None,
                        },
                    });
                }
            }
        });
    });

    rsx! {
        Router::<Route> {}
    }
}
