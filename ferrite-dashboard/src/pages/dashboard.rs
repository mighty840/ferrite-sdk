use crate::api::types::*;
use crate::components::DeviceCard;
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn DashboardPage() -> Element {
    let devices = use_signal(|| {
        vec![
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
        ]
    });

    let online_count = devices()
        .iter()
        .filter(|d| d.status == DeviceStatus::Online)
        .count();
    let degraded_count = devices()
        .iter()
        .filter(|d| d.status == DeviceStatus::Degraded)
        .count();
    let offline_count = devices()
        .iter()
        .filter(|d| d.status == DeviceStatus::Offline)
        .count();
    let total_count = devices().len();

    rsx! {
        div {
            class: "p-6 lg:p-8 max-w-[1400px] mx-auto",
            // Header
            div {
                class: "mb-8 animate-fade-in",
                h1 {
                    class: "text-2xl font-semibold text-gray-100",
                    "Overview"
                }
                p {
                    class: "mt-1 text-sm text-gray-500",
                    "Fleet status and recent activity"
                }
            }

            // Stat cards
            div {
                class: "grid grid-cols-2 lg:grid-cols-4 gap-4 mb-8",
                StatCard {
                    label: "Total Devices",
                    value: total_count.to_string(),
                    accent: "text-gray-300",
                    bg: "bg-surface-800",
                    border: "border-surface-700",
                }
                StatCard {
                    label: "Online",
                    value: online_count.to_string(),
                    accent: "text-emerald-400",
                    bg: "bg-emerald-500/5",
                    border: "border-emerald-500/20",
                }
                StatCard {
                    label: "Degraded",
                    value: degraded_count.to_string(),
                    accent: "text-amber-400",
                    bg: "bg-amber-500/5",
                    border: "border-amber-500/20",
                }
                StatCard {
                    label: "Offline",
                    value: offline_count.to_string(),
                    accent: "text-red-400",
                    bg: "bg-red-500/5",
                    border: "border-red-500/20",
                }
            }

            // Devices section
            div {
                class: "mb-8",
                div {
                    class: "flex items-center justify-between mb-4",
                    h2 {
                        class: "text-sm font-medium text-gray-400 uppercase tracking-wider",
                        "Devices"
                    }
                    Link {
                        to: Route::Devices {},
                        class: "text-xs text-ferrite-500 hover:text-ferrite-400 font-medium transition-colors",
                        "View all"
                    }
                }
                div {
                    class: "grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-4 gap-4",
                    for device in devices() {
                        DeviceCard { device: device }
                    }
                }
            }

            // Activity feed
            div {
                class: "bg-surface-900 rounded-xl border border-surface-700",
                div {
                    class: "px-5 py-4 border-b border-surface-700",
                    h2 {
                        class: "text-sm font-medium text-gray-400 uppercase tracking-wider",
                        "Recent Activity"
                    }
                }
                div {
                    class: "divide-y divide-surface-750",
                    ActivityItem {
                        color: "bg-emerald-500",
                        title: "Device online",
                        description: "Temperature Sensor A reconnected",
                        time: "2m ago",
                    }
                    ActivityItem {
                        color: "bg-amber-500",
                        title: "Warning fault",
                        description: "Motor Controller B: high vibration detected",
                        time: "15m ago",
                    }
                    ActivityItem {
                        color: "bg-red-500",
                        title: "Device offline",
                        description: "Pressure Sensor D lost connection",
                        time: "1h ago",
                    }
                    ActivityItem {
                        color: "bg-blue-500",
                        title: "Firmware update",
                        description: "Gateway Hub C updated to v3.0.1",
                        time: "3h ago",
                    }
                }
            }
        }
    }
}

#[component]
fn StatCard(
    label: &'static str,
    value: String,
    accent: &'static str,
    bg: &'static str,
    border: &'static str,
) -> Element {
    rsx! {
        div {
            class: "rounded-xl border p-5 {bg} {border} animate-fade-in",
            p {
                class: "text-[10px] font-semibold text-gray-500 uppercase tracking-widest mb-2",
                "{label}"
            }
            p {
                class: "text-3xl font-mono font-bold {accent}",
                "{value}"
            }
        }
    }
}

#[component]
fn ActivityItem(
    color: &'static str,
    title: &'static str,
    description: &'static str,
    time: &'static str,
) -> Element {
    rsx! {
        div {
            class: "px-5 py-3.5 flex items-center space-x-4 hover:bg-surface-850 transition-colors",
            div {
                class: "flex-shrink-0",
                div {
                    class: "h-2 w-2 rounded-full {color}"
                }
            }
            div {
                class: "flex-1 min-w-0",
                p {
                    class: "text-sm font-medium text-gray-200",
                    "{title}"
                }
                p {
                    class: "text-xs text-gray-500",
                    "{description}"
                }
            }
            span {
                class: "text-[10px] text-gray-600 font-mono flex-shrink-0",
                "{time}"
            }
        }
    }
}
