use dioxus::prelude::*;

#[component]
pub fn Loading(message: Option<String>) -> Element {
    let msg = message.unwrap_or_else(|| "Loading...".to_string());

    rsx! {
        div {
            class: "flex flex-col items-center justify-center py-12",
            div {
                class: "relative",
                svg {
                    class: "animate-spin h-10 w-10 text-iotai-500",
                    fill: "none",
                    view_box: "0 0 24 24",
                    circle {
                        class: "opacity-25",
                        cx: "12",
                        cy: "12",
                        r: "10",
                        stroke: "currentColor",
                        stroke_width: "4",
                    }
                    path {
                        class: "opacity-75",
                        fill: "currentColor",
                        d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                    }
                }
            }
            p {
                class: "mt-4 text-sm text-gray-500",
                "{msg}"
            }
        }
    }
}

#[component]
pub fn ErrorDisplay(message: String) -> Element {
    rsx! {
        div {
            class: "rounded-lg bg-red-50 border border-red-200 p-4 my-4",
            div {
                class: "flex items-center",
                svg {
                    class: "h-5 w-5 text-red-500 mr-3",
                    fill: "none",
                    view_box: "0 0 24 24",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                    }
                }
                p {
                    class: "text-sm text-red-700",
                    "{message}"
                }
            }
        }
    }
}
