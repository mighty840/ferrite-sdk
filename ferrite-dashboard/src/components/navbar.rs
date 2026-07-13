use crate::auth::AuthState;
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn Navbar() -> Element {
    let mut mobile_open = use_signal(|| false);
    let auth_state = use_context::<Signal<AuthState>>();

    let user_initial = match &auth_state() {
        AuthState::Authenticated { user, .. } => user
            .name
            .chars()
            .next()
            .unwrap_or('U')
            .to_uppercase()
            .to_string(),
        _ => "U".to_string(),
    };

    let user_name = match &auth_state() {
        AuthState::Authenticated { user, .. } => user.name.clone(),
        _ => "User".to_string(),
    };

    rsx! {
        // Mobile top bar
        div {
            class: "lg:hidden fixed top-0 left-0 right-0 z-50 bg-surface-900 border-b border-surface-700 h-14 flex items-center px-4",
            button {
                class: "text-gray-400 hover:text-ferrite-400 p-1 transition-colors",
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
            div {
                class: "ml-3 flex items-center space-x-2",
                div {
                    class: "h-7 w-7 rounded-md bg-ferrite-600/20 border border-ferrite-600/30 flex items-center justify-center",
                    span {
                        class: "text-ferrite-500 font-mono font-bold text-xs",
                        "Fe"
                    }
                }
                span {
                    class: "text-gray-100 font-semibold text-sm tracking-tight",
                    "Ferrite"
                }
            }
        }

        // Sidebar
        aside {
            class: if mobile_open() {
                "fixed inset-0 z-40 flex lg:static lg:inset-auto"
            } else {
                "hidden lg:flex lg:static"
            },

            // Overlay for mobile
            if mobile_open() {
                div {
                    class: "fixed inset-0 bg-black/60 lg:hidden",
                    onclick: move |_| mobile_open.set(false),
                }
            }

            nav {
                class: "relative z-50 flex flex-col w-64 min-h-screen bg-surface-900 border-r border-surface-700",
                // Logo
                div {
                    class: "h-16 flex items-center px-5 border-b border-surface-700",
                    div {
                        class: "flex items-center space-x-2.5",
                        div {
                            class: "h-8 w-8 rounded-lg bg-ferrite-600/20 border border-ferrite-600/30 flex items-center justify-center",
                            span {
                                class: "text-ferrite-500 font-mono font-bold text-sm",
                                "Fe"
                            }
                        }
                        div {
                            span {
                                class: "text-gray-100 font-semibold text-base tracking-tight block leading-tight",
                                "Ferrite"
                            }
                            span {
                                class: "text-gray-500 text-[10px] font-mono uppercase tracking-widest block",
                                "observability"
                            }
                        }
                    }
                }

                // Navigation links
                div {
                    class: "flex-1 py-4 px-3 space-y-1 overflow-y-auto",
                    div {
                        class: "mb-3 px-3",
                        p {
                            class: "text-[10px] font-semibold text-gray-500 uppercase tracking-widest",
                            "Monitor"
                        }
                    }
                    SidebarLink {
                        to: Route::Dashboard {},
                        label: "Overview",
                        icon_path: "M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6",
                        on_click: move |_| mobile_open.set(false),
                    }
                    SidebarLink {
                        to: Route::Devices {},
                        label: "Devices",
                        icon_path: "M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z",
                        on_click: move |_| mobile_open.set(false),
                    }
                    SidebarLink {
                        to: Route::Metrics {},
                        label: "Metrics",
                        icon_path: "M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z",
                        on_click: move |_| mobile_open.set(false),
                    }

                    SidebarLink {
                        to: Route::Fleet {},
                        label: "Fleet",
                        icon_path: "M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10",
                        on_click: move |_| mobile_open.set(false),
                    }
                    SidebarLink {
                        to: Route::Ota {},
                        label: "OTA",
                        icon_path: "M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M9 19l3 3m0 0l3-3m-3 3V10",
                        on_click: move |_| mobile_open.set(false),
                    }
                    SidebarLink {
                        to: Route::Compare {},
                        label: "Compare",
                        icon_path: "M9 17V7m0 10a2 2 0 01-2 2H5a2 2 0 01-2-2V7a2 2 0 012-2h2a2 2 0 012 2m0 10a2 2 0 002 2h2a2 2 0 002-2M9 7a2 2 0 012-2h2a2 2 0 012 2m0 10V7m0 10a2 2 0 002 2h2a2 2 0 002-2V7a2 2 0 00-2-2h-2a2 2 0 00-2 2",
                        on_click: move |_| mobile_open.set(false),
                    }

                    div {
                        class: "mt-6 mb-3 px-3",
                        p {
                            class: "text-[10px] font-semibold text-gray-500 uppercase tracking-widest",
                            "Diagnostics"
                        }
                    }
                    SidebarLink {
                        to: Route::Faults {},
                        label: "Faults",
                        icon_path: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z",
                        on_click: move |_| mobile_open.set(false),
                    }
                    SidebarLink {
                        to: Route::Crashes {},
                        label: "Crashes",
                        icon_path: "M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z",
                        on_click: move |_| mobile_open.set(false),
                    }

                    div {
                        class: "mt-6 mb-3 px-3",
                        p {
                            class: "text-[10px] font-semibold text-gray-500 uppercase tracking-widest",
                            "System"
                        }
                    }
                    SidebarLink {
                        to: Route::Settings {},
                        label: "Settings",
                        icon_path: "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z",
                        on_click: move |_| mobile_open.set(false),
                    }
                }

                // User section at bottom
                div {
                    class: "border-t border-surface-700 p-3",
                    div {
                        class: "flex items-center space-x-3 px-2 py-2 rounded-lg",
                        div {
                            class: "h-8 w-8 rounded-full bg-ferrite-600/20 border border-ferrite-600/30 flex items-center justify-center text-ferrite-400 text-sm font-mono font-semibold flex-shrink-0",
                            "{user_initial}"
                        }
                        div {
                            class: "flex-1 min-w-0",
                            p {
                                class: "text-sm font-medium text-gray-200 truncate",
                                "{user_name}"
                            }
                            p {
                                class: "text-[10px] text-gray-500 font-mono",
                                "operator"
                            }
                        }
                        button {
                            class: "text-gray-500 hover:text-ferrite-400 transition-colors p-1",
                            title: "Sign out",
                            onclick: move |_| {
                                if let Some(storage) = web_sys::window()
                                    .and_then(|w| w.session_storage().ok())
                                    .flatten()
                                {
                                    let _ = storage.remove_item("ferrite_auth_token");
                                    let _ = storage.remove_item("ferrite_auth_type");
                                    let _ = storage.remove_item("ferrite_auth_user");
                                }
                                if let Some(window) = web_sys::window() {
                                    let _ = window.location().reload();
                                }
                            },
                            svg {
                                class: "h-4 w-4",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SidebarLink(
    to: Route,
    label: &'static str,
    icon_path: &'static str,
    on_click: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        Link {
            to: to,
            class: "flex items-center space-x-3 px-3 py-2 rounded-lg text-sm text-gray-400 hover:text-gray-100 hover:bg-surface-750 transition-all duration-150 group",
            onclick: move |e| on_click.call(e),
            svg {
                class: "h-5 w-5 text-gray-500 group-hover:text-ferrite-500 transition-colors flex-shrink-0",
                fill: "none",
                view_box: "0 0 24 24",
                stroke: "currentColor",
                stroke_width: "1.5",
                path {
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    d: icon_path,
                }
            }
            span { "{label}" }
        }
    }
}
