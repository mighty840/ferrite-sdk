use crate::api::types::{Device, DeviceStatus};
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn DeviceCard(device: Device) -> Element {
    let (status_color, status_dot, status_glow) = match device.status {
        DeviceStatus::Online => ("text-emerald-400", "bg-emerald-400", true),
        DeviceStatus::Offline => ("text-red-400", "bg-red-400", false),
        DeviceStatus::Degraded => ("text-amber-400", "bg-amber-400", false),
        DeviceStatus::Unknown => ("text-gray-500", "bg-gray-500", false),
    };

    let last_seen = device.last_seen.format("%H:%M UTC").to_string();
    let device_id = device.id.clone();

    rsx! {
        Link {
            to: Route::DeviceDetail { id: device_id },
            class: "block group",
            div {
                class: "bg-surface-900 rounded-xl border border-surface-700 p-5 hover:border-ferrite-600/40 hover:bg-surface-850 transition-all duration-200",
                // Header row
                div {
                    class: "flex items-start justify-between mb-4",
                    div {
                        class: "flex items-center space-x-3",
                        div {
                            class: "h-9 w-9 rounded-lg bg-ferrite-600/10 border border-ferrite-600/20 flex items-center justify-center",
                            svg {
                                class: "h-4 w-4 text-ferrite-500",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "1.5",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z"
                                }
                            }
                        }
                        div {
                            h3 {
                                class: "text-sm font-medium text-gray-200 group-hover:text-gray-100 transition-colors",
                                "{device.name}"
                            }
                            p {
                                class: "text-[10px] text-gray-500 font-mono uppercase tracking-wider",
                                "{device.device_type}"
                            }
                        }
                    }
                    div {
                        class: "flex items-center space-x-1.5",
                        div {
                            class: if status_glow {
                                "h-2 w-2 rounded-full {status_dot} pulse-glow"
                            } else {
                                "h-2 w-2 rounded-full {status_dot}"
                            },
                        }
                        span {
                            class: "text-[10px] font-medium {status_color} uppercase tracking-wider",
                            "{device.status}"
                        }
                    }
                }
                // Details
                div {
                    class: "space-y-2",
                    div {
                        class: "flex justify-between text-xs",
                        span { class: "text-gray-500", "Firmware" }
                        span { class: "text-gray-300 font-mono", "{device.firmware_version}" }
                    }
                    div {
                        class: "flex justify-between text-xs",
                        span { class: "text-gray-500", "Last seen" }
                        span { class: "text-gray-400 font-mono", "{last_seen}" }
                    }
                    if let Some(ip) = &device.ip_address {
                        div {
                            class: "flex justify-between text-xs",
                            span { class: "text-gray-500", "IP" }
                            span { class: "text-gray-400 font-mono", "{ip}" }
                        }
                    }
                }
                if !device.tags.is_empty() {
                    div {
                        class: "mt-3 flex flex-wrap gap-1.5",
                        for tag in &device.tags {
                            span {
                                class: "inline-block bg-surface-750 text-gray-400 text-[10px] px-2 py-0.5 rounded font-mono",
                                "{tag}"
                            }
                        }
                    }
                }
            }
        }
    }
}
