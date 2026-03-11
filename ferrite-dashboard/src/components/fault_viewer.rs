use crate::api::types::FaultEvent;
use dioxus::prelude::*;

#[component]
pub fn FaultViewer(fault: FaultEvent) -> Element {
    let type_name = fault.fault_type_name();
    let (severity_bg, severity_text, severity_border, severity_dot) = match fault.fault_type {
        0 => (
            "bg-red-500/5",
            "text-red-400",
            "border-red-500/20",
            "bg-red-500",
        ),
        1 | 2 => (
            "bg-amber-500/5",
            "text-amber-400",
            "border-amber-500/20",
            "bg-amber-500",
        ),
        _ => (
            "bg-blue-500/5",
            "text-blue-400",
            "border-blue-500/20",
            "bg-blue-500",
        ),
    };

    let pc_hex = format!("0x{:08X}", fault.pc);
    let lr_hex = format!("0x{:08X}", fault.lr);
    let symbol = fault.symbol.as_deref().unwrap_or("unknown");

    rsx! {
        div {
            class: "bg-surface-900 rounded-xl border {severity_border} p-5 {severity_bg}",
            div {
                class: "flex items-start space-x-4",
                div {
                    class: "flex-shrink-0 mt-1",
                    div { class: "h-2.5 w-2.5 rounded-full {severity_dot}" }
                }
                div {
                    class: "flex-1 min-w-0",
                    div {
                        class: "flex items-center justify-between",
                        div {
                            class: "flex items-center space-x-2",
                            h3 {
                                class: "text-sm font-mono font-semibold text-gray-100",
                                "{type_name}"
                            }
                            span {
                                class: "text-[10px] font-semibold {severity_text} uppercase tracking-wider",
                                "PC {pc_hex}"
                            }
                        }
                        span {
                            class: "text-[10px] text-gray-600 font-mono",
                            "{fault.created_at}"
                        }
                    }
                    p {
                        class: "mt-1.5 text-sm text-gray-400 font-mono",
                        "Symbol: {symbol} | LR: {lr_hex}"
                    }
                    div {
                        class: "mt-3 flex items-center space-x-4 text-xs text-gray-500",
                        span {
                            class: "font-mono",
                            "{fault.device_id}"
                        }
                    }
                }
            }
        }
    }
}
