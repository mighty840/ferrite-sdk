use crate::api::types::TraceEntry;
use dioxus::prelude::*;

#[component]
pub fn TraceViewer(traces: Vec<TraceEntry>) -> Element {
    rsx! {
        div {
            class: "bg-gray-900 rounded-lg shadow overflow-hidden",
            div {
                class: "flex items-center justify-between px-4 py-2 bg-gray-800 border-b border-gray-700",
                h3 {
                    class: "text-sm font-medium text-gray-200",
                    "Trace Log"
                }
                div {
                    class: "flex items-center space-x-2",
                    span {
                        class: "text-xs text-gray-400",
                        "{traces.len()} entries"
                    }
                }
            }
            div {
                class: "overflow-y-auto max-h-96 font-mono text-xs",
                if traces.is_empty() {
                    div {
                        class: "p-4 text-gray-500 text-center",
                        "No trace entries available"
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
        "WARN" => "text-yellow-400",
        "INFO" => "text-green-400",
        "DEBUG" => "text-blue-400",
        "TRACE" => "text-gray-500",
        _ => "text-gray-400",
    };

    let timestamp = trace.timestamp.format("%H:%M:%S%.3f").to_string();
    let span_info = trace
        .span_id
        .as_ref()
        .map(|s| format!(" [{}]", s))
        .unwrap_or_default();

    rsx! {
        div {
            class: "px-4 py-1 hover:bg-gray-800 border-b border-gray-800 flex",
            span {
                class: "text-gray-500 mr-3 flex-shrink-0 w-24",
                "{timestamp}"
            }
            span {
                class: "{level_color} mr-3 flex-shrink-0 w-12 uppercase font-bold",
                "{trace.level}"
            }
            span {
                class: "text-purple-400 mr-3 flex-shrink-0",
                "{trace.module}{span_info}"
            }
            span {
                class: "text-gray-300 break-all",
                "{trace.message}"
            }
        }
    }
}
