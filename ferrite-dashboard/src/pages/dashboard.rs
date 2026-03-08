use dioxus::prelude::*;
use dioxus_router::prelude::*;
use crate::api::types::*;
use crate::components::DeviceCard;
use crate::Route;

#[component]
pub fn DashboardPage() -> Element {
    let devices = use_signal(|| vec![
        Device {
            id: "dev-001".into(),
            name: "Temperature Sensor A".into(),
            device_type: "sensor".into(),
            status: DeviceStatus::Online,
            firmware_version: "1.4.2".into(),
            last_seen: chrono::Utc::now(),
            ip_address: Some("192.168.1.10".into()),
            tags: vec!["production".into(), "floor-1".into()],
        },
        Device {
            id: "dev-002".into(),
            name: "Motor Controller B".into(),
            device_type: "actuator".into(),
            status: DeviceStatus::Degraded,
            firmware_version: "2.1.0".into(),
            last_seen: chrono::Utc::now(),
            ip_address: Some("192.168.1.11".into()),
            tags: vec!["production".into()],
        },
        Device {
            id: "dev-003".into(),
            name: "Gateway Hub C".into(),
            device_type: "gateway".into(),
            status: DeviceStatus::Online,
            firmware_version: "3.0.1".into(),
            last_seen: chrono::Utc::now(),
            ip_address: Some("192.168.1.1".into()),
            tags: vec!["infrastructure".into()],
        },
        Device {
            id: "dev-004".into(),
            name: "Pressure Sensor D".into(),
            device_type: "sensor".into(),
            status: DeviceStatus::Offline,
            firmware_version: "1.2.0".into(),
            last_seen: chrono::Utc::now(),
            ip_address: None,
            tags: vec!["staging".into()],
        },
    ]);

    let online_count = devices().iter().filter(|d| d.status == DeviceStatus::Online).count();
    let degraded_count = devices().iter().filter(|d| d.status == DeviceStatus::Degraded).count();
    let offline_count = devices().iter().filter(|d| d.status == DeviceStatus::Offline).count();
    let total_count = devices().len();

    rsx! {
        div {
            class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8",
            // Header
            div {
                class: "mb-8",
                h1 {
                    class: "text-2xl font-bold text-gray-900",
                    "Dashboard"
                }
                p {
                    class: "mt-1 text-sm text-gray-500",
                    "Overview of your IoT device fleet"
                }
            }

            // Summary cards
            div {
                class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-8",
                SummaryCard {
                    title: "Total Devices",
                    value: total_count.to_string(),
                    icon_path: "M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z",
                    color: "blue",
                }
                SummaryCard {
                    title: "Online",
                    value: online_count.to_string(),
                    icon_path: "M5 13l4 4L19 7",
                    color: "green",
                }
                SummaryCard {
                    title: "Degraded",
                    value: degraded_count.to_string(),
                    icon_path: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z",
                    color: "yellow",
                }
                SummaryCard {
                    title: "Offline",
                    value: offline_count.to_string(),
                    icon_path: "M6 18L18 6M6 6l12 12",
                    color: "red",
                }
            }

            // Recent devices grid
            div {
                class: "mb-6",
                div {
                    class: "flex items-center justify-between mb-4",
                    h2 {
                        class: "text-lg font-semibold text-gray-900",
                        "Devices"
                    }
                    Link {
                        to: Route::Devices {},
                        class: "text-sm text-ferrite-600 hover:text-ferrite-800 font-medium",
                        "View all"
                    }
                }
                div {
                    class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4",
                    for device in devices() {
                        DeviceCard { device: device }
                    }
                }
            }

            // Recent activity
            div {
                class: "bg-white rounded-lg shadow border border-gray-200",
                div {
                    class: "px-6 py-4 border-b border-gray-200",
                    h2 {
                        class: "text-lg font-semibold text-gray-900",
                        "Recent Activity"
                    }
                }
                div {
                    class: "divide-y divide-gray-200",
                    ActivityItem {
                        icon_color: "text-green-500",
                        title: "Device online",
                        description: "Temperature Sensor A reconnected",
                        time: "2 minutes ago",
                    }
                    ActivityItem {
                        icon_color: "text-yellow-500",
                        title: "Warning fault",
                        description: "Motor Controller B: high vibration detected",
                        time: "15 minutes ago",
                    }
                    ActivityItem {
                        icon_color: "text-red-500",
                        title: "Device offline",
                        description: "Pressure Sensor D lost connection",
                        time: "1 hour ago",
                    }
                    ActivityItem {
                        icon_color: "text-blue-500",
                        title: "Firmware update",
                        description: "Gateway Hub C updated to v3.0.1",
                        time: "3 hours ago",
                    }
                }
            }
        }
    }
}

#[component]
fn SummaryCard(
    title: &'static str,
    value: String,
    icon_path: &'static str,
    color: &'static str,
) -> Element {
    let (bg, icon_color, text_color) = match color {
        "green" => ("bg-green-50", "text-green-500", "text-green-700"),
        "yellow" => ("bg-yellow-50", "text-yellow-500", "text-yellow-700"),
        "red" => ("bg-red-50", "text-red-500", "text-red-700"),
        _ => ("bg-blue-50", "text-blue-500", "text-blue-700"),
    };

    rsx! {
        div {
            class: "bg-white rounded-lg shadow border border-gray-200 p-5",
            div {
                class: "flex items-center",
                div {
                    class: "flex-shrink-0 p-3 rounded-lg {bg}",
                    svg {
                        class: "h-6 w-6 {icon_color}",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: icon_path,
                        }
                    }
                }
                div {
                    class: "ml-4",
                    p {
                        class: "text-sm font-medium text-gray-500",
                        "{title}"
                    }
                    p {
                        class: "text-2xl font-bold {text_color}",
                        "{value}"
                    }
                }
            }
        }
    }
}

#[component]
fn ActivityItem(
    icon_color: &'static str,
    title: &'static str,
    description: &'static str,
    time: &'static str,
) -> Element {
    rsx! {
        div {
            class: "px-6 py-4 flex items-center space-x-4",
            div {
                class: "flex-shrink-0",
                svg {
                    class: "h-5 w-5 {icon_color}",
                    fill: "currentColor",
                    view_box: "0 0 20 20",
                    circle { cx: "10", cy: "10", r: "5" }
                }
            }
            div {
                class: "flex-1 min-w-0",
                p {
                    class: "text-sm font-medium text-gray-900",
                    "{title}"
                }
                p {
                    class: "text-sm text-gray-500",
                    "{description}"
                }
            }
            span {
                class: "text-xs text-gray-400 flex-shrink-0",
                "{time}"
            }
        }
    }
}
