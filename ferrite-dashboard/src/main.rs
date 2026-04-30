pub mod api;
pub mod auth;
pub mod components;
pub mod hooks;
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
        #[route("/devices/register")]
        Register {},
        #[route("/devices/:id")]
        DeviceDetail { id: String },
        #[route("/faults")]
        Faults {},
        #[route("/crashes")]
        Crashes {},
        #[route("/crashes/:id")]
        CrashDetail { id: String },
        #[route("/metrics")]
        Metrics {},
        #[route("/fleet")]
        Fleet {},
        #[route("/compare")]
        Compare {},
        #[route("/ota")]
        Ota {},
        #[route("/ota/campaigns/new")]
        CampaignNew {},
        #[route("/ota/campaigns/:id")]
        CampaignDetail { id: String },
        #[route("/settings")]
        Settings {},
    #[end_layout]
    #[route("/callback?:code&:state")]
    Callback { code: String, state: String },
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
                    p { class: "text-gray-500 text-sm font-mono", "initializing..." }
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
            div {
                class: "flex min-h-screen bg-surface-950",
                Navbar {}
                main {
                    class: "flex-1 lg:ml-0 mt-14 lg:mt-0 overflow-auto dot-grid",
                    Outlet::<Route> {}
                }
            }
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

/// Register device page route handler.
#[component]
fn Register() -> Element {
    rsx! { RegisterPage {} }
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

/// Crashes list page route handler.
#[component]
fn Crashes() -> Element {
    rsx! { CrashesPage {} }
}

/// Crash detail page route handler.
#[component]
fn CrashDetail(id: String) -> Element {
    rsx! { CrashDetailPage { id: id } }
}

/// Metrics overview page route handler.
#[component]
fn Metrics() -> Element {
    rsx! { MetricsPage {} }
}

/// Fleet overview page route handler.
#[component]
fn Fleet() -> Element {
    rsx! { FleetPage {} }
}

/// Device comparison page route handler.
#[component]
fn Compare() -> Element {
    rsx! { ComparePage {} }
}

/// OTA campaigns list route handler.
#[component]
fn Ota() -> Element {
    rsx! { OtaPage {} }
}

/// New campaign wizard route handler.
#[component]
fn CampaignNew() -> Element {
    rsx! { CampaignNewPage {} }
}

/// Campaign detail route handler.
#[component]
fn CampaignDetail(id: String) -> Element {
    rsx! { CampaignDetailPage { id } }
}

/// Settings page route handler.
#[component]
fn Settings() -> Element {
    rsx! { SettingsPage {} }
}

/// OIDC callback route handler.
#[component]
fn Callback(code: String, state: String) -> Element {
    rsx! { CallbackPage { code, state } }
}

/// 404 not found page.
#[component]
fn NotFound(route: Vec<String>) -> Element {
    let path = route.join("/");
    rsx! {
        div {
            class: "min-h-screen flex items-center justify-center bg-surface-950",
            div {
                class: "text-center",
                h1 {
                    class: "text-7xl font-mono font-bold text-surface-700 mb-4",
                    "404"
                }
                p {
                    class: "text-lg text-gray-500 mb-2",
                    "Route not found"
                }
                p {
                    class: "text-sm text-gray-600 font-mono mb-8",
                    "/{path}"
                }
                Link {
                    to: Route::Dashboard {},
                    class: "inline-flex items-center px-4 py-2 bg-ferrite-600 text-white rounded-lg hover:bg-ferrite-500 text-sm font-medium transition-colors duration-150",
                    "Back to Overview"
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
            let client = api::ApiClient::new(&api::client::api_url());
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
