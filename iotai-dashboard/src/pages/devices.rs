use dioxus::prelude::*;
use crate::api::types::*;
use crate::components::DeviceCard;

#[component]
pub fn DevicesPage() -> Element {
    let mut search = use_signal(|| String::new());
    let mut status_filter = use_signal(|| "all".to_string());

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
        Device {
            id: "dev-005".into(),
            name: "Humidity Sensor E".into(),
            device_type: "sensor".into(),
            status: DeviceStatus::Online,
            firmware_version: "1.4.2".into(),
            last_seen: chrono::Utc::now(),
            ip_address: Some("192.168.1.15".into()),
            tags: vec!["production".into(), "floor-2".into()],
        },
        Device {
            id: "dev-006".into(),
            name: "Valve Controller F".into(),
            device_type: "actuator".into(),
            status: DeviceStatus::Unknown,
            firmware_version: "1.0.0".into(),
            last_seen: chrono::Utc::now(),
            ip_address: None,
            tags: vec!["staging".into()],
        },
    ]);

    let filtered: Vec<Device> = devices()
        .into_iter()
        .filter(|d| {
            let matches_search = search().is_empty()
                || d.name.to_lowercase().contains(&search().to_lowercase())
                || d.id.to_lowercase().contains(&search().to_lowercase());
            let matches_status = match status_filter().as_str() {
                "online" => d.status == DeviceStatus::Online,
                "offline" => d.status == DeviceStatus::Offline,
                "degraded" => d.status == DeviceStatus::Degraded,
                _ => true,
            };
            matches_search && matches_status
        })
        .collect();

    rsx! {
        div {
            class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8",
            div {
                class: "mb-6",
                h1 {
                    class: "text-2xl font-bold text-gray-900",
                    "Devices"
                }
                p {
                    class: "mt-1 text-sm text-gray-500",
                    "Manage and monitor your IoT devices"
                }
            }

            // Filters
            div {
                class: "flex flex-col sm:flex-row gap-4 mb-6",
                div {
                    class: "flex-1",
                    div {
                        class: "relative",
                        svg {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-gray-400",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                            }
                        }
                        input {
                            class: "w-full pl-10 pr-4 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-iotai-500 focus:border-iotai-500 outline-none",
                            r#type: "text",
                            placeholder: "Search devices...",
                            value: "{search}",
                            oninput: move |e| search.set(e.value()),
                        }
                    }
                }
                select {
                    class: "px-4 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-iotai-500 focus:border-iotai-500 outline-none bg-white",
                    value: "{status_filter}",
                    onchange: move |e| status_filter.set(e.value()),
                    option { value: "all", "All statuses" }
                    option { value: "online", "Online" }
                    option { value: "degraded", "Degraded" }
                    option { value: "offline", "Offline" }
                }
            }

            // Results count
            p {
                class: "text-sm text-gray-500 mb-4",
                "Showing {filtered.len()} device(s)"
            }

            // Device grid
            div {
                class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4",
                for device in filtered {
                    DeviceCard { device: device }
                }
            }
        }
    }
}
