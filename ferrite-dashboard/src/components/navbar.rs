use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn Navbar() -> Element {
    let mut mobile_open = use_signal(|| false);

    rsx! {
        nav {
            class: "bg-ferrite-900 shadow-lg",
            div {
                class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8",
                div {
                    class: "flex items-center justify-between h-16",
                    div {
                        class: "flex items-center",
                        div {
                            class: "flex-shrink-0",
                            span {
                                class: "text-white text-xl font-bold tracking-tight",
                                "ferrite"
                            }
                            span {
                                class: "text-ferrite-300 text-sm ml-2",
                                "Dashboard"
                            }
                        }
                        div {
                            class: "hidden md:block ml-10",
                            div {
                                class: "flex items-baseline space-x-4",
                                NavLink { to: Route::Dashboard {}, label: "Dashboard" }
                                NavLink { to: Route::Devices {}, label: "Devices" }
                                NavLink { to: Route::Faults {}, label: "Faults" }
                                NavLink { to: Route::Metrics {}, label: "Metrics" }
                                NavLink { to: Route::Settings {}, label: "Settings" }
                            }
                        }
                    }
                    div {
                        class: "hidden md:flex items-center space-x-4",
                        div {
                            class: "relative",
                            span {
                                class: "absolute top-0 right-0 block h-2 w-2 rounded-full bg-green-400 ring-2 ring-ferrite-900"
                            }
                            svg {
                                class: "h-6 w-6 text-gray-300",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"
                                }
                            }
                        }
                        div {
                            class: "h-8 w-8 rounded-full bg-ferrite-500 flex items-center justify-center text-white text-sm font-medium",
                            "U"
                        }
                    }
                    // Mobile menu button
                    div {
                        class: "md:hidden",
                        button {
                            class: "text-gray-300 hover:text-white p-2",
                            onclick: move |_| mobile_open.set(!mobile_open()),
                            svg {
                                class: "h-6 w-6",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M4 6h16M4 12h16M4 18h16"
                                }
                            }
                        }
                    }
                }
            }
            // Mobile menu
            if mobile_open() {
                div {
                    class: "md:hidden",
                    div {
                        class: "px-2 pt-2 pb-3 space-y-1 sm:px-3",
                        NavLink { to: Route::Dashboard {}, label: "Dashboard" }
                        NavLink { to: Route::Devices {}, label: "Devices" }
                        NavLink { to: Route::Faults {}, label: "Faults" }
                        NavLink { to: Route::Metrics {}, label: "Metrics" }
                        NavLink { to: Route::Settings {}, label: "Settings" }
                    }
                }
            }
        }
    }
}

#[component]
fn NavLink(to: Route, label: &'static str) -> Element {
    rsx! {
        Link {
            to: to,
            class: "text-gray-300 hover:bg-ferrite-700 hover:text-white px-3 py-2 rounded-md text-sm font-medium transition-colors duration-150",
            "{label}"
        }
    }
}
