use dioxus::prelude::*;

#[component]
pub fn Loading(message: Option<String>) -> Element {
    let msg = message.unwrap_or_else(|| "Loading...".to_string());

    rsx! {
        div {
            class: "flex items-center justify-center py-16",
            div {
                class: "text-center",
                div {
                    class: "animate-spin h-6 w-6 border-2 border-ferrite-500 border-t-transparent rounded-full mx-auto mb-3"
                }
                p { class: "text-gray-500 text-sm font-mono", "{msg}" }
            }
        }
    }
}

#[component]
pub fn ErrorDisplay(message: String) -> Element {
    rsx! {
        div {
            class: "rounded-lg bg-red-500/10 border border-red-500/20 p-4 my-4",
            div {
                class: "flex items-center",
                svg {
                    class: "h-5 w-5 text-red-400 mr-3 flex-shrink-0",
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
                    class: "text-sm text-red-400",
                    "{message}"
                }
            }
        }
    }
}
