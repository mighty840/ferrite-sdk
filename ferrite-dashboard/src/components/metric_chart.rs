use crate::api::types::MetricRow;
use dioxus::prelude::*;

/// Extract a numeric value from the metric's value_json.
fn extract_value(value_json: &str) -> Option<f64> {
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(value_json) {
        // Try common formats: {"counter": N}, {"gauge": N}, {"value": N}, {"histogram": {"min": N, "max": N}}
        if let Some(v) = parsed.get("counter").and_then(|v| v.as_f64()) {
            return Some(v);
        }
        if let Some(v) = parsed.get("gauge").and_then(|v| v.as_f64()) {
            return Some(v);
        }
        if let Some(v) = parsed.get("value").and_then(|v| v.as_f64()) {
            return Some(v);
        }
        if let Some(hist) = parsed.get("histogram") {
            // Use midpoint of min/max
            let min = hist.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let max = hist.get("max").and_then(|v| v.as_f64()).unwrap_or(0.0);
            return Some((min + max) / 2.0);
        }
        // Try top-level number
        if let Some(v) = parsed.as_f64() {
            return Some(v);
        }
    }
    None
}

/// SVG time-series chart for a single metric key.
#[component]
pub fn MetricChart(metrics: Vec<MetricRow>, metric_key: String) -> Element {
    let filtered: Vec<&MetricRow> = metrics.iter().filter(|m| m.key == metric_key).collect();

    if filtered.is_empty() {
        return rsx! {
            div {
                class: "bg-surface-900 rounded-xl border border-surface-700 p-6 text-center text-sm text-gray-500",
                "No data for \"{metric_key}\""
            }
        };
    }

    let data_points: Vec<(usize, f64)> = filtered
        .iter()
        .enumerate()
        .filter_map(|(i, m)| extract_value(&m.value_json).map(|v| (i, v)))
        .collect();

    if data_points.is_empty() {
        return rsx! {
            div {
                class: "bg-surface-900 rounded-xl border border-surface-700 p-6 text-center text-sm text-gray-500",
                "No numeric values for \"{metric_key}\""
            }
        };
    }

    // Chart dimensions
    let width: f64 = 600.0;
    let height: f64 = 200.0;
    let padding_x: f64 = 50.0;
    let padding_y: f64 = 20.0;
    let chart_w = width - padding_x * 2.0;
    let chart_h = height - padding_y * 2.0;

    let values: Vec<f64> = data_points.iter().map(|(_, v)| *v).collect();
    let min_val = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_val = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = if (max_val - min_val).abs() < f64::EPSILON {
        1.0
    } else {
        max_val - min_val
    };

    let n = data_points.len();
    let x_step = if n > 1 {
        chart_w / (n - 1) as f64
    } else {
        chart_w
    };

    // Build SVG path for the line
    let mut line_path = String::new();
    let mut area_path = String::new();
    for (idx, (_, val)) in data_points.iter().enumerate() {
        let x = padding_x + idx as f64 * x_step;
        let y = padding_y + chart_h - ((val - min_val) / range * chart_h);
        if idx == 0 {
            line_path.push_str(&format!("M{x:.1},{y:.1}"));
            area_path.push_str(&format!("M{:.1},{:.1} L{x:.1},{y:.1}", padding_x, padding_y + chart_h));
        } else {
            line_path.push_str(&format!(" L{x:.1},{y:.1}"));
            area_path.push_str(&format!(" L{x:.1},{y:.1}"));
        }
    }
    // Close area path
    let last_x = padding_x + (n.saturating_sub(1)) as f64 * x_step;
    area_path.push_str(&format!(
        " L{:.1},{:.1} Z",
        last_x,
        padding_y + chart_h
    ));

    // Latest value for display
    let latest_val = values.last().copied().unwrap_or(0.0);
    let latest_str = if latest_val.fract() == 0.0 {
        format!("{}", latest_val as i64)
    } else {
        format!("{:.2}", latest_val)
    };

    // Y-axis labels (min, mid, max)
    let min_label = format_compact(min_val);
    let max_label = format_compact(max_val);
    let mid_label = format_compact((min_val + max_val) / 2.0);

    // Time labels (first and last created_at)
    let first_time = filtered.first().map(|m| short_time(&m.created_at)).unwrap_or_default();
    let last_time = filtered.last().map(|m| short_time(&m.created_at)).unwrap_or_default();

    let svg_width = format!("{width}");
    let svg_height = format!("{height}");
    let viewbox = format!("0 0 {width} {height}");

    rsx! {
        div {
            class: "bg-surface-900 rounded-xl border border-surface-700 p-5",
            div {
                class: "flex items-center justify-between mb-3",
                div {
                    h3 {
                        class: "text-sm font-mono font-semibold text-gray-200",
                        "{metric_key}"
                    }
                    p {
                        class: "text-[10px] text-gray-500 font-mono mt-0.5",
                        "{filtered.len()} data points"
                    }
                }
                div {
                    class: "text-right",
                    p {
                        class: "text-lg font-mono font-bold text-ferrite-400",
                        "{latest_str}"
                    }
                    p {
                        class: "text-[10px] text-gray-500 font-mono",
                        "latest"
                    }
                }
            }
            svg {
                width: svg_width,
                height: svg_height,
                view_box: viewbox,
                class: "w-full h-auto",
                // Grid lines
                line {
                    x1: "{padding_x}",
                    y1: "{padding_y}",
                    x2: "{padding_x}",
                    y2: "{padding_y + chart_h}",
                    stroke: "#2a2a2a",
                    stroke_width: "1",
                }
                line {
                    x1: "{padding_x}",
                    y1: "{padding_y + chart_h}",
                    x2: "{padding_x + chart_w}",
                    y2: "{padding_y + chart_h}",
                    stroke: "#2a2a2a",
                    stroke_width: "1",
                }
                // Mid grid line
                line {
                    x1: "{padding_x}",
                    y1: "{padding_y + chart_h / 2.0}",
                    x2: "{padding_x + chart_w}",
                    y2: "{padding_y + chart_h / 2.0}",
                    stroke: "#1a1a1a",
                    stroke_width: "1",
                    stroke_dasharray: "4",
                }
                // Area fill
                path {
                    d: "{area_path}",
                    fill: "url(#chartGradient)",
                    opacity: "0.3",
                }
                // Line
                path {
                    d: "{line_path}",
                    fill: "none",
                    stroke: "#e05d44",
                    stroke_width: "2",
                    stroke_linejoin: "round",
                    stroke_linecap: "round",
                }
                // Y-axis labels
                text {
                    x: "{padding_x - 5.0}",
                    y: "{padding_y + 4.0}",
                    fill: "#6b7280",
                    font_size: "10",
                    text_anchor: "end",
                    font_family: "monospace",
                    "{max_label}"
                }
                text {
                    x: "{padding_x - 5.0}",
                    y: "{padding_y + chart_h / 2.0 + 4.0}",
                    fill: "#6b7280",
                    font_size: "10",
                    text_anchor: "end",
                    font_family: "monospace",
                    "{mid_label}"
                }
                text {
                    x: "{padding_x - 5.0}",
                    y: "{padding_y + chart_h + 4.0}",
                    fill: "#6b7280",
                    font_size: "10",
                    text_anchor: "end",
                    font_family: "monospace",
                    "{min_label}"
                }
                // X-axis time labels
                text {
                    x: "{padding_x}",
                    y: "{padding_y + chart_h + 16.0}",
                    fill: "#6b7280",
                    font_size: "9",
                    text_anchor: "start",
                    font_family: "monospace",
                    "{first_time}"
                }
                text {
                    x: "{padding_x + chart_w}",
                    y: "{padding_y + chart_h + 16.0}",
                    fill: "#6b7280",
                    font_size: "9",
                    text_anchor: "end",
                    font_family: "monospace",
                    "{last_time}"
                }
                // Gradient definition
                defs {
                    linearGradient {
                        id: "chartGradient",
                        x1: "0",
                        y1: "0",
                        x2: "0",
                        y2: "1",
                        stop {
                            offset: "0%",
                            stop_color: "#e05d44",
                            stop_opacity: "0.4",
                        }
                        stop {
                            offset: "100%",
                            stop_color: "#e05d44",
                            stop_opacity: "0.0",
                        }
                    }
                }
            }
        }
    }
}

/// Compact number formatting.
fn format_compact(v: f64) -> String {
    if v.abs() >= 1_000_000.0 {
        format!("{:.1}M", v / 1_000_000.0)
    } else if v.abs() >= 1_000.0 {
        format!("{:.1}K", v / 1_000.0)
    } else if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{:.1}", v)
    }
}

/// Extract a short time from ISO datetime (e.g. "2025-03-12 14:30:00" -> "14:30")
fn short_time(datetime: &str) -> String {
    if let Some(t) = datetime.split(' ').nth(1) {
        t.get(..5).unwrap_or(t).to_string()
    } else {
        datetime.to_string()
    }
}
