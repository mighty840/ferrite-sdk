use dioxus::prelude::*;
use dioxus_router::prelude::*;
use crate::api::types::{Device, DeviceStatus};
use crate::Route;

#[component]
pub fn DeviceCard(device: Device) -> Element {
    let status_color = match device.status {
        DeviceStatus::Online => "bg-green-100 text-green-800",
        DeviceStatus::Offline => "bg-red-100 text-red-800",
        DeviceStatus::Degraded => "bg-yellow-100 text-yellow-800",
        DeviceStatus::Unknown => "bg-gray-100 text-gray-800",
    };

    let status_dot = match device.status {
        DeviceStatus::Online => "bg-green-400",
        DeviceStatus::Offline => "bg-red-400",
        DeviceStatus::Degraded => "bg-yellow-400",
        DeviceStatus::Unknown => "bg-gray-400",
    };

    let last_seen = device.last_seen.format("%Y-%m-%d %H:%M UTC").to_string();
    let device_id = device.id.clone();

    rsx! {
        Link {
            to: Route::DeviceDetail { id: device_id },
            class: "block",
            div {
                class: "bg-white rounded-lg shadow hover:shadow-md transition-shadow duration-200 p-6 border border-gray-200",
                div {
                    class: "flex items-center justify-between mb-4",
                    div {
                        class: "flex items-center space-x-3",
                        div {
                            class: "h-10 w-10 rounded-lg bg-iotai-100 flex items-center justify-center",
                            svg {
                                class: "h-6 w-6 text-iotai-600",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z"
                                }
                            }
                        }
                        div {
                            h3 {
                                class: "text-sm font-semibold text-gray-900",
                                "{device.name}"
                            }
                            p {
                                class: "text-xs text-gray-500",
                                "{device.device_type}"
                            }
                        }
                    }
                    span {
                        class: "inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium {status_color}",
                        span {
                            class: "h-2 w-2 rounded-full {status_dot} mr-1.5"
                        }
                        "{device.status}"
                    }
                }
                div {
                    class: "space-y-2",
                    div {
                        class: "flex justify-between text-xs",
                        span { class: "text-gray-500", "Firmware" }
                        span { class: "text-gray-700 font-mono", "{device.firmware_version}" }
                    }
                    div {
                        class: "flex justify-between text-xs",
                        span { class: "text-gray-500", "Last seen" }
                        span { class: "text-gray-700", "{last_seen}" }
                    }
                    if let Some(ip) = &device.ip_address {
                        div {
                            class: "flex justify-between text-xs",
                            span { class: "text-gray-500", "IP" }
                            span { class: "text-gray-700 font-mono", "{ip}" }
                        }
                    }
                }
                if !device.tags.is_empty() {
                    div {
                        class: "mt-3 flex flex-wrap gap-1",
                        for tag in &device.tags {
                            span {
                                class: "inline-block bg-gray-100 text-gray-600 text-xs px-2 py-0.5 rounded",
                                "{tag}"
                            }
                        }
                    }
                }
            }
        }
    }
}
