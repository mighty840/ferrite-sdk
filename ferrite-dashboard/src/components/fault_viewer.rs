use crate::api::types::{FaultEvent, FaultSeverity};
use dioxus::prelude::*;

#[component]
pub fn FaultViewer(fault: FaultEvent) -> Element {
    let (severity_bg, severity_text, severity_border, severity_dot) = match fault.severity {
        FaultSeverity::Critical => (
            "bg-red-500/5",
            "text-red-400",
            "border-red-500/20",
            "bg-red-500",
        ),
        FaultSeverity::Warning => (
            "bg-amber-500/5",
            "text-amber-400",
            "border-amber-500/20",
            "bg-amber-500",
        ),
        FaultSeverity::Info => (
            "bg-blue-500/5",
            "text-blue-400",
            "border-blue-500/20",
            "bg-blue-500",
        ),
    };

    let timestamp = fault.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
    let resolved_text = if fault.resolved {
        let at = fault
            .resolved_at
            .map(|t| t.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        format!("Resolved {}", at)
    } else {
        "Unresolved".to_string()
    };

    let resolved_color = if fault.resolved {
        "text-emerald-400"
    } else {
        "text-red-400"
    };

    rsx! {
        div {
            class: "bg-surface-900 rounded-xl border {severity_border} p-5 {severity_bg}",
            div {
                class: "flex items-start space-x-4",
                div {
                    class: "flex-shrink-0 mt-1",
                    div {
                        class: "h-2.5 w-2.5 rounded-full {severity_dot}"
                    }
                }
                div {
                    class: "flex-1 min-w-0",
                    div {
                        class: "flex items-center justify-between",
                        div {
                            class: "flex items-center space-x-2",
                            h3 {
                                class: "text-sm font-mono font-semibold text-gray-100",
                                "{fault.code}"
                            }
                            span {
                                class: "text-[10px] font-semibold {severity_text} uppercase tracking-wider",
                                "{fault.severity}"
                            }
                        }
                        span {
                            class: "text-[10px] text-gray-600 font-mono",
                            "{timestamp}"
                        }
                    }
                    p {
                        class: "mt-1.5 text-sm text-gray-400",
                        "{fault.message}"
                    }
                    div {
                        class: "mt-3 flex items-center justify-between",
                        div {
                            class: "flex items-center space-x-4 text-xs text-gray-500",
                            span {
                                class: "font-mono",
                                "{fault.device_name}"
                            }
                            span {
                                class: "text-gray-600",
                                "{fault.device_id}"
                            }
                        }
                        span {
                            class: "text-[10px] font-medium font-mono {resolved_color}",
                            "{resolved_text}"
                        }
                    }
                }
            }
        }
    }
}
