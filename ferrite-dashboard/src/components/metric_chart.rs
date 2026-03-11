use crate::api::types::MetricRow;
use dioxus::prelude::*;

#[component]
pub fn MetricChart(title: String, entries: Vec<MetricRow>, color: Option<String>) -> Element {
    let stroke = color.unwrap_or_else(|| "#f97316".to_string());

    // Try to extract numeric values from value_json
    let values: Vec<f64> = entries
        .iter()
        .filter_map(|e| {
            // Try to parse various json formats: {"counter":42}, {"gauge":3.14}, etc.
            serde_json::from_str::<serde_json::Value>(&e.value_json)
                .ok()
                .and_then(|v| {
                    if let Some(n) = v.as_f64() {
                        Some(n)
                    } else if let Some(obj) = v.as_object() {
                        obj.values().next().and_then(|v| v.as_f64())
                    } else {
                        None
                    }
                })
        })
        .collect();

    let (path_d, area_d, min_val, max_val) = if values.is_empty() {
        (
            "M 0 50 L 300 50".to_string(),
            "M 0 50 L 300 50 L 300 80 L 0 80 Z".to_string(),
            0.0,
            100.0,
        )
    } else {
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let range = if (max - min).abs() < 0.001 {
            1.0
        } else {
            max - min
        };

        let width = 300.0;
        let height = 80.0;
        let padding = 5.0;
        let step = width / (values.len() as f64 - 1.0).max(1.0);

        let points: Vec<(f64, f64)> = values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let x = i as f64 * step;
                let y = height - padding - ((v - min) / range) * (height - 2.0 * padding);
                (x, y)
            })
            .collect();

        let line_d = points
            .iter()
            .enumerate()
            .map(|(i, (x, y))| {
                if i == 0 {
                    format!("M {:.1} {:.1}", x, y)
                } else {
                    format!("L {:.1} {:.1}", x, y)
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        let area = format!(
            "{} L {:.1} {:.1} L 0 {:.1} Z",
            line_d,
            points.last().map(|(x, _)| *x).unwrap_or(width),
            height,
            height
        );

        (line_d, area, min, max)
    };

    let latest_value = values
        .last()
        .map(|v| format!("{:.1}", v))
        .unwrap_or_else(|| "--".to_string());

    // Generate unique gradient ID from title
    let grad_id = format!("grad-{}", title.replace(' ', "-").to_lowercase());
    let grad_url = format!("url(#{})", grad_id);

    rsx! {
        div {
            class: "bg-surface-900 rounded-xl border border-surface-700 p-4",
            div {
                class: "flex items-center justify-between mb-3",
                h3 {
                    class: "text-xs font-medium text-gray-400 uppercase tracking-wider",
                    "{title}"
                }
                div {
                    class: "text-right",
                    span {
                        class: "text-xl font-mono font-bold text-gray-100",
                        "{latest_value}"
                    }
                }
            }
            svg {
                class: "w-full",
                view_box: "0 0 300 80",
                preserve_aspect_ratio: "none",
                defs {
                    linearGradient {
                        id: "{grad_id}",
                        x1: "0",
                        y1: "0",
                        x2: "0",
                        y2: "1",
                        stop { offset: "0%", stop_color: "{stroke}", stop_opacity: "0.25" }
                        stop { offset: "100%", stop_color: "{stroke}", stop_opacity: "0.0" }
                    }
                }
                line { x1: "0", y1: "20", x2: "300", y2: "20", stroke: "#1e222e", stroke_width: "0.5" }
                line { x1: "0", y1: "40", x2: "300", y2: "40", stroke: "#1e222e", stroke_width: "0.5" }
                line { x1: "0", y1: "60", x2: "300", y2: "60", stroke: "#1e222e", stroke_width: "0.5" }
                path {
                    d: "{area_d}",
                    fill: "{grad_url}",
                }
                path {
                    d: "{path_d}",
                    fill: "none",
                    stroke: "{stroke}",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                }
            }
            div {
                class: "flex justify-between text-[10px] text-gray-600 mt-1 font-mono",
                span { "{min_val:.1}" }
                span { "{max_val:.1}" }
            }
        }
    }
}
