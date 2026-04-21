use crate::api::types::*;
use crate::auth::AuthState;
use crate::components::{ErrorDisplay, Loading};
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn CampaignNewPage() -> Element {
    let auth_state = use_context::<Signal<AuthState>>();
    let nav = use_navigator();

    // Wizard step: 0 = pick firmware, 1 = configure
    let mut step = use_signal(|| 0u8);
    let mut selected_firmware: Signal<Option<FirmwareArtifact>> = use_signal(|| None);
    let mut name = use_signal(|| String::new());
    let mut strategy = use_signal(|| "immediate".to_string());
    let mut rollout_pct = use_signal(|| 100i64);
    let mut activate_now = use_signal(|| true);
    let mut submitting = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);

    let firmware_resource = use_resource(move || async move {
        let client = crate::api::client::authenticated_client(&auth_state());
        client.list_firmware().await
    });

    let firmware_binding = firmware_resource.read();

    rsx! {
        div {
            class: "p-6 lg:p-8 max-w-2xl mx-auto",

            // Header
            div { class: "mb-6 animate-fade-in",
                div { class: "flex items-center gap-2 text-sm text-gray-500 mb-3",
                    Link { to: Route::Ota {}, class: "hover:text-gray-300 transition-colors", "OTA" }
                    span { "/" }
                    span { class: "text-gray-300", "New Campaign" }
                }
                h1 { class: "text-2xl font-semibold text-gray-100", "New OTA Campaign" }
                p { class: "mt-1 text-sm text-gray-500", "Deploy a firmware update to your fleet" }
            }

            // Step indicator
            div { class: "flex items-center gap-3 mb-6",
                StepDot { n: 1, active: step() == 0, done: step() > 0, label: "Select Firmware" }
                div { class: "flex-1 h-px bg-surface-700" }
                StepDot { n: 2, active: step() == 1, done: false, label: "Configure" }
            }

            if let Some(msg) = error_msg() {
                div {
                    class: "mb-4 px-4 py-3 bg-red-500/10 border border-red-500/20 rounded-lg text-sm text-red-400",
                    "{msg}"
                }
            }

            // ── Step 0: Firmware picker ──────────────────────────────────────
            if step() == 0 {
                div {
                    match &*firmware_binding {
                        None => rsx! { Loading {} },
                        Some(Err(e)) => rsx! { ErrorDisplay { message: e.to_string() } },
                        Some(Ok(artifacts)) => {
                            if artifacts.is_empty() {
                                rsx! {
                                    div {
                                        class: "bg-surface-900 rounded-xl border border-surface-700 p-10 text-center",
                                        p { class: "text-sm text-gray-500 mb-2", "No firmware artifacts uploaded yet" }
                                        p { class: "text-xs text-gray-600 font-mono",
                                            "ferrite firmware upload --file firmware.bin --version 1.0.0"
                                        }
                                    }
                                }
                            } else {
                                rsx! {
                                    div {
                                        class: "space-y-2 mb-6",
                                        for artifact in artifacts.iter() {
                                            FirmwareCard {
                                                artifact: artifact.clone(),
                                                selected: selected_firmware().as_ref().map(|f| f.id) == Some(artifact.id),
                                                on_select: move |a: FirmwareArtifact| {
                                                    selected_firmware.set(Some(a));
                                                },
                                            }
                                        }
                                    }
                                    div { class: "flex justify-end",
                                        button {
                                            class: "px-5 py-2 bg-ferrite-600 text-white rounded-lg hover:bg-ferrite-500 text-sm font-medium transition-colors disabled:opacity-40",
                                            disabled: selected_firmware().is_none(),
                                            onclick: move |_| {
                                                if selected_firmware().is_some() {
                                                    step.set(1);
                                                }
                                            },
                                            "Next: Configure →"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Step 1: Configuration ────────────────────────────────────────
            if step() == 1 {
                div {
                    class: "bg-surface-900 rounded-xl border border-surface-700 p-5 mb-4",

                    // Selected firmware summary
                    if let Some(f) = selected_firmware() {
                        div {
                            class: "mb-5 p-3 bg-ferrite-600/10 border border-ferrite-600/20 rounded-lg",
                            p { class: "text-xs text-gray-400 mb-0.5", "Selected firmware" }
                            p { class: "text-sm font-mono text-ferrite-400 font-semibold",
                                "{f.version} — {f.size_display()} — #{f.build_id}"
                            }
                        }
                    }

                    div { class: "space-y-4",
                        // Campaign name
                        div {
                            label {
                                class: "block text-xs font-semibold text-gray-400 uppercase tracking-wider mb-1.5",
                                "Campaign Name"
                            }
                            input {
                                class: "w-full px-3 py-2.5 bg-surface-800 border border-surface-600 rounded-lg text-sm text-gray-200 placeholder-gray-600 focus:ring-2 focus:ring-ferrite-500/40 focus:border-ferrite-600 outline-none transition-all font-mono",
                                placeholder: "e.g. rollout-v2.1-prod",
                                value: "{name}",
                                oninput: move |e| name.set(e.value()),
                            }
                        }

                        // Strategy
                        div {
                            label {
                                class: "block text-xs font-semibold text-gray-400 uppercase tracking-wider mb-1.5",
                                "Rollout Strategy"
                            }
                            select {
                                class: "w-full px-3 py-2.5 bg-surface-800 border border-surface-600 rounded-lg text-sm text-gray-300 focus:ring-2 focus:ring-ferrite-500/40 focus:border-ferrite-600 outline-none transition-all",
                                value: "{strategy}",
                                onchange: move |e| strategy.set(e.value()),
                                option { value: "immediate", "Immediate — push to all devices at once" }
                                option { value: "canary", "Canary — push to a percentage first" }
                                option { value: "scheduled", "Scheduled — push on activation" }
                            }
                        }

                        // Rollout percent (canary only)
                        if strategy() == "canary" {
                            div {
                                label {
                                    class: "block text-xs font-semibold text-gray-400 uppercase tracking-wider mb-1.5",
                                    "Canary Rollout Percent: {rollout_pct}%"
                                }
                                input {
                                    class: "w-full accent-ferrite-500",
                                    r#type: "range",
                                    min: "1",
                                    max: "100",
                                    value: "{rollout_pct}",
                                    oninput: move |e| {
                                        if let Ok(v) = e.value().parse::<i64>() {
                                            rollout_pct.set(v);
                                        }
                                    },
                                }
                                div { class: "flex justify-between text-xs text-gray-600 mt-1",
                                    span { "1%" }
                                    span { "50%" }
                                    span { "100%" }
                                }
                            }
                        }

                        // Activate immediately
                        div { class: "flex items-center gap-3",
                            input {
                                r#type: "checkbox",
                                class: "accent-ferrite-500 h-4 w-4",
                                id: "activate-now",
                                checked: activate_now(),
                                onchange: move |e| activate_now.set(e.checked()),
                            }
                            label {
                                r#for: "activate-now",
                                class: "text-sm text-gray-300 cursor-pointer",
                                "Activate campaign immediately after creation"
                            }
                        }
                    }
                }

                div { class: "flex items-center justify-between",
                    button {
                        class: "px-4 py-2 text-sm text-gray-400 hover:text-gray-200 transition-colors",
                        onclick: move |_| step.set(0),
                        "← Back"
                    }
                    button {
                        class: "px-5 py-2 bg-ferrite-600 text-white rounded-lg hover:bg-ferrite-500 text-sm font-medium transition-colors disabled:opacity-40",
                        disabled: name().trim().is_empty() || submitting(),
                        onclick: move |_| {
                            let Some(firmware) = selected_firmware() else { return; };
                            let campaign_name = name().trim().to_string();
                            if campaign_name.is_empty() { return; }

                            submitting.set(true);
                            error_msg.set(None);

                            let strat = strategy();
                            let pct = rollout_pct();
                            let do_activate = activate_now();
                            let nav = nav.clone();

                            let client = crate::api::client::authenticated_client(&auth_state());
                            spawn(async move {
                                match client.create_campaign(
                                    &campaign_name,
                                    firmware.id,
                                    &firmware.version,
                                    &strat,
                                    pct,
                                ).await {
                                    Ok(campaign) => {
                                        let cid = campaign.id;
                                        if do_activate {
                                            let _ = client.activate_campaign(cid).await;
                                        }
                                        nav.push(Route::CampaignDetail { id: cid.to_string() });
                                    }
                                    Err(e) => {
                                        error_msg.set(Some(e.to_string()));
                                        submitting.set(false);
                                    }
                                }
                            });
                        },
                        if submitting() { "Creating…" } else { "Create Campaign" }
                    }
                }
            }
        }
    }
}

#[component]
fn FirmwareCard(
    artifact: FirmwareArtifact,
    selected: bool,
    on_select: EventHandler<FirmwareArtifact>,
) -> Element {
    let border = if selected {
        "border-ferrite-500 bg-ferrite-600/10"
    } else {
        "border-surface-600 hover:border-surface-500 bg-surface-900"
    };

    rsx! {
        button {
            class: "w-full text-left p-4 rounded-xl border {border} transition-all",
            onclick: move |_| on_select.call(artifact.clone()),
            div { class: "flex items-center justify-between",
                div {
                    p { class: "text-sm font-mono font-semibold text-ferrite-400", "{artifact.version}" }
                    p { class: "text-xs text-gray-500 mt-0.5",
                        "Build #{artifact.build_id} · {artifact.size_display()} · {artifact.created_at.get(..10).unwrap_or(&artifact.created_at)}"
                    }
                }
                if selected {
                    div { class: "h-5 w-5 rounded-full bg-ferrite-500 flex items-center justify-center flex-shrink-0",
                        svg { class: "h-3 w-3 text-white", fill: "none", view_box: "0 0 24 24", stroke: "currentColor", stroke_width: "3",
                            path { stroke_linecap: "round", stroke_linejoin: "round", d: "M5 13l4 4L19 7" }
                        }
                    }
                } else {
                    div { class: "h-5 w-5 rounded-full border border-surface-500 flex-shrink-0" }
                }
            }
        }
    }
}

#[component]
fn StepDot(n: u8, active: bool, done: bool, label: String) -> Element {
    let (dot_class, text_class) = if active {
        ("bg-ferrite-600 border-ferrite-500 text-white", "text-ferrite-400")
    } else if done {
        ("bg-green-500/20 border-green-500/40 text-green-400", "text-green-400")
    } else {
        ("bg-surface-800 border-surface-600 text-gray-500", "text-gray-500")
    };
    rsx! {
        div { class: "flex flex-col items-center gap-1",
            div {
                class: "h-7 w-7 rounded-full border-2 {dot_class} flex items-center justify-center text-xs font-bold",
                "{n}"
            }
            span { class: "text-[10px] font-medium {text_class} hidden sm:block", "{label}" }
        }
    }
}
