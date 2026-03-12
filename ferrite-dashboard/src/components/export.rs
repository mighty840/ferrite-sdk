use dioxus::prelude::*;

/// Trigger a browser file download with the given content.
fn download_file(filename: &str, content: &str, mime: &str) {
    let js = format!(
        r#"
        (function() {{
            var blob = new Blob([`{content}`], {{ type: '{mime}' }});
            var url = URL.createObjectURL(blob);
            var a = document.createElement('a');
            a.href = url;
            a.download = '{filename}';
            document.body.appendChild(a);
            a.click();
            document.body.removeChild(a);
            URL.revokeObjectURL(url);
        }})();
        "#,
        content = content
            .replace('\\', "\\\\")
            .replace('`', "\\`")
            .replace("${", "\\${"),
        mime = mime,
        filename = filename,
    );
    let _ = js_sys::eval(&js);
}

/// Export button pair (CSV + JSON) for any serializable data.
#[component]
pub fn ExportButtons(csv_data: String, json_data: String, filename_base: String) -> Element {
    let csv_filename = format!("{filename_base}.csv");
    let json_filename = format!("{filename_base}.json");

    rsx! {
        div {
            class: "flex items-center space-x-2",
            button {
                class: "inline-flex items-center px-3 py-1.5 bg-surface-800 text-gray-400 border border-surface-700 rounded-lg text-xs font-mono hover:border-ferrite-500/40 hover:text-gray-200 transition-colors",
                onclick: move |_| {
                    download_file(&csv_filename, &csv_data, "text/csv");
                },
                svg {
                    class: "h-3.5 w-3.5 mr-1.5",
                    fill: "none",
                    view_box: "0 0 24 24",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                    }
                }
                "CSV"
            }
            button {
                class: "inline-flex items-center px-3 py-1.5 bg-surface-800 text-gray-400 border border-surface-700 rounded-lg text-xs font-mono hover:border-ferrite-500/40 hover:text-gray-200 transition-colors",
                onclick: move |_| {
                    download_file(&json_filename, &json_data, "application/json");
                },
                svg {
                    class: "h-3.5 w-3.5 mr-1.5",
                    fill: "none",
                    view_box: "0 0 24 24",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                    }
                }
                "JSON"
            }
        }
    }
}

/// Convert faults to CSV string.
pub fn faults_to_csv(faults: &[crate::api::types::FaultEvent]) -> String {
    let mut csv = String::from("id,device_id,fault_type,pc,lr,cfsr,hfsr,symbol,created_at\n");
    for f in faults {
        csv.push_str(&format!(
            "{},{},{},0x{:08X},0x{:08X},0x{:08X},0x{:08X},{},{}\n",
            f.id,
            f.device_id,
            f.fault_type_name(),
            f.pc,
            f.lr,
            f.cfsr,
            f.hfsr,
            f.symbol.as_deref().unwrap_or(""),
            f.created_at,
        ));
    }
    csv
}

/// Convert metrics to CSV string.
pub fn metrics_to_csv(metrics: &[crate::api::types::MetricRow]) -> String {
    let mut csv = String::from("id,device_id,key,metric_type,value,timestamp_ticks,created_at\n");
    for m in metrics {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            m.id, m.device_id, m.key, m.metric_type, m.value_json, m.timestamp_ticks, m.created_at,
        ));
    }
    csv
}
