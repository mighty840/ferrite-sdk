use dioxus::prelude::*;
use crate::api::types::{FaultEvent, FaultSeverity};

#[component]
pub fn FaultViewer(fault: FaultEvent) -> Element {
    let severity_badge = match fault.severity {
        FaultSeverity::Critical => "bg-red-100 text-red-800 border-red-200",
        FaultSeverity::Warning => "bg-yellow-100 text-yellow-800 border-yellow-200",
        FaultSeverity::Info => "bg-blue-100 text-blue-800 border-blue-200",
    };

    let severity_icon_color = match fault.severity {
        FaultSeverity::Critical => "text-red-500",
        FaultSeverity::Warning => "text-yellow-500",
        FaultSeverity::Info => "text-blue-500",
    };

    let timestamp = fault.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string();
    let resolved_text = if fault.resolved {
        let at = fault
            .resolved_at
            .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "unknown time".to_string());
        format!("Resolved at {}", at)
    } else {
        "Unresolved".to_string()
    };

    let resolved_color = if fault.resolved {
        "text-green-600"
    } else {
        "text-red-600"
    };

    rsx! {
        div {
            class: "bg-white rounded-lg shadow border border-gray-200 p-5",
            div {
                class: "flex items-start space-x-4",
                div {
                    class: "flex-shrink-0 mt-0.5",
                    svg {
                        class: "h-6 w-6 {severity_icon_color}",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z"
                        }
                    }
                }
                div {
                    class: "flex-1 min-w-0",
                    div {
                        class: "flex items-center justify-between",
                        div {
                            class: "flex items-center space-x-2",
                            h3 {
                                class: "text-sm font-semibold text-gray-900",
                                "{fault.code}"
                            }
                            span {
                                class: "inline-flex items-center px-2 py-0.5 rounded text-xs font-medium border {severity_badge}",
                                "{fault.severity}"
                            }
                        }
                        span {
                            class: "text-xs text-gray-500",
                            "{timestamp}"
                        }
                    }
                    p {
                        class: "mt-1 text-sm text-gray-700",
                        "{fault.message}"
                    }
                    div {
                        class: "mt-3 flex items-center justify-between",
                        div {
                            class: "flex items-center space-x-4 text-xs text-gray-500",
                            span {
                                "Device: "
                                span {
                                    class: "font-medium text-gray-700",
                                    "{fault.device_name}"
                                }
                            }
                            span {
                                "ID: "
                                span {
                                    class: "font-mono text-gray-600",
                                    "{fault.device_id}"
                                }
                            }
                        }
                        span {
                            class: "text-xs font-medium {resolved_color}",
                            "{resolved_text}"
                        }
                    }
                }
            }
        }
    }
}
