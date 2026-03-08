pub mod api;
pub mod auth;
pub mod components;
pub mod pages;

use dioxus::prelude::*;
use dioxus_router::prelude::*;

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

/// Layout wrapper that includes the navbar on every page.
#[component]
fn AppLayout() -> Element {
    rsx! {
        Navbar {}
        Outlet::<Route> {}
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

/// WASM entry point.
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn main() {
    dioxus::launch(App);
}

/// Root application component.
#[component]
fn App() -> Element {
    rsx! {
        Router::<Route> {}
    }
}
