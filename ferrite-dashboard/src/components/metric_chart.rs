use crate::api::types::MetricEntry;
use dioxus::prelude::*;

#[component]
pub fn MetricChart(title: String, entries: Vec<MetricEntry>, color: Option<String>) -> Element {
    let stroke = color.unwrap_or_else(|| "#3b82f6".to_string());

    // Build SVG path from metric entries
    let (path_d, min_val, max_val) = if entries.is_empty() {
        ("M 0 50 L 300 50".to_string(), 0.0, 100.0)
    } else {
        let values: Vec<f64> = entries.iter().map(|e| e.value).collect();
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

        let points: Vec<String> = values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let x = i as f64 * step;
                let y = height - padding - ((v - min) / range) * (height - 2.0 * padding);
                format!("{:.1} {:.1}", x, y)
            })
            .collect();

        let d = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if i == 0 {
                    format!("M {}", p)
                } else {
                    format!("L {}", p)
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        (d, min, max)
    };

    let unit = entries.first().map(|e| e.unit.clone()).unwrap_or_default();
    let latest_value = entries
        .last()
        .map(|e| format!("{:.1}", e.value))
        .unwrap_or_else(|| "--".to_string());

    rsx! {
        div {
            class: "bg-white rounded-lg shadow border border-gray-200 p-4",
            div {
                class: "flex items-center justify-between mb-3",
                h3 {
                    class: "text-sm font-medium text-gray-700",
                    "{title}"
                }
                div {
                    class: "text-right",
                    span {
                        class: "text-lg font-bold text-gray-900",
                        "{latest_value}"
                    }
                    span {
                        class: "text-xs text-gray-500 ml-1",
                        "{unit}"
                    }
                }
            }
            svg {
                class: "w-full",
                view_box: "0 0 300 80",
                preserve_aspect_ratio: "none",
                // Grid lines
                line { x1: "0", y1: "20", x2: "300", y2: "20", stroke: "#e5e7eb", stroke_width: "0.5" }
                line { x1: "0", y1: "40", x2: "300", y2: "40", stroke: "#e5e7eb", stroke_width: "0.5" }
                line { x1: "0", y1: "60", x2: "300", y2: "60", stroke: "#e5e7eb", stroke_width: "0.5" }
                // Data line
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
                class: "flex justify-between text-xs text-gray-400 mt-1",
                span { "{min_val:.1} {unit}" }
                span { "{max_val:.1} {unit}" }
            }
        }
    }
}
