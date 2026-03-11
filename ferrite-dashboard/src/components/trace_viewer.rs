use crate::api::types::TraceEntry;
use dioxus::prelude::*;

#[component]
pub fn TraceViewer(traces: Vec<TraceEntry>) -> Element {
    rsx! {
        div {
            class: "bg-surface-950 rounded-xl border border-surface-700 overflow-hidden",
            div {
                class: "flex items-center justify-between px-4 py-3 bg-surface-900 border-b border-surface-700",
                div {
                    class: "flex items-center space-x-2",
                    div {
                        class: "h-2 w-2 rounded-full bg-emerald-500 pulse-glow"
                    }
                    h3 {
                        class: "text-xs font-medium text-gray-400 uppercase tracking-wider",
                        "Trace Log"
                    }
                }
                span {
                    class: "text-[10px] text-gray-600 font-mono",
                    "{traces.len()} entries"
                }
            }
            div {
                class: "overflow-y-auto max-h-96 font-mono text-xs",
                if traces.is_empty() {
                    div {
                        class: "p-8 text-gray-600 text-center text-sm",
                        "No trace entries"
                    }
                }
                for trace in &traces {
                    {trace_line(trace)}
                }
            }
        }
    }
}

fn trace_line(trace: &TraceEntry) -> Element {
    let level_color = match trace.level.to_uppercase().as_str() {
        "ERROR" => "text-red-400",
        "WARN" => "text-amber-400",
        "INFO" => "text-emerald-400",
        "DEBUG" => "text-blue-400",
        "TRACE" => "text-gray-600",
        _ => "text-gray-500",
    };

    let span_info = trace
        .span_id
        .as_ref()
        .map(|s| format!(" [{}]", s))
        .unwrap_or_default();

    rsx! {
        div {
            class: "px-4 py-1.5 hover:bg-surface-900 border-b border-surface-900/50 flex items-start",
            span {
                class: "text-gray-600 mr-3 flex-shrink-0 w-24 tabular-nums",
                "{trace.timestamp}"
            }
            span {
                class: "{level_color} mr-3 flex-shrink-0 w-12 uppercase font-bold",
                "{trace.level}"
            }
            span {
                class: "text-ferrite-500/70 mr-3 flex-shrink-0",
                "{trace.module}{span_info}"
            }
            span {
                class: "text-gray-400 break-all",
                "{trace.message}"
            }
        }
    }
}
