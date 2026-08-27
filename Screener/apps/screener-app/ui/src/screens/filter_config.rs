use dioxus::prelude::*;

use crate::api::ApiClient;
use crate::dto::FilterConfigDto;
use crate::state::AppState;

#[component]
pub fn FilterConfig() -> Element {
    let state = use_context::<Signal<AppState>>();
    let api = use_context::<ApiClient>();

    // `use_signal`/`use_resource`/`use_effect` must run unconditionally on
    // every render (Dioxus, like React, indexes hooks by call order) — so
    // `config`/`save_status` are created up front with a placeholder value
    // and populated from `loaded` via an effect, rather than being created
    // lazily inside a match arm once data arrives.
    let mut config = use_signal(FilterConfigDto::default);
    let mut save_status = use_signal(String::new);
    let mut populated = use_signal(|| false);

    let loaded = use_resource({
        let api = api.clone();
        move || {
            let api = api.clone();
            let token = state.read().token.clone().unwrap_or_default();
            async move { api.get_filter_config(&token).await.ok() }
        }
    });

    use_effect(move || {
        if populated() {
            return;
        }
        if let Some(Some(fetched)) = &*loaded.read() {
            config.set(fetched.clone());
            populated.set(true);
        }
    });

    if loaded.read().is_none() {
        return rsx! { section { h2 { "Filter Config" } p { "Loading..." } } };
    }
    if matches!(&*loaded.read(), Some(None)) {
        return rsx! { section { h2 { "Filter Config" } p { "Failed to load." } } };
    }

    rsx! {
        section {
            h2 { "Filter Config" }

            h3 { "Phase 1 (Daily)" }
            label {
                input {
                    r#type: "checkbox",
                    checked: config.read().phase1.enable_close_gt_sma10,
                    oninput: move |evt| config.write().phase1.enable_close_gt_sma10 = evt.checked(),
                }
                "Close > SMA10"
            }
            label {
                input {
                    r#type: "checkbox",
                    checked: config.read().phase1.enable_sma10_gt_sma20,
                    oninput: move |evt| config.write().phase1.enable_sma10_gt_sma20 = evt.checked(),
                }
                "SMA10 > SMA20"
            }
            label {
                input {
                    r#type: "checkbox",
                    checked: config.read().phase1.enable_sma20_gt_sma50,
                    oninput: move |evt| config.write().phase1.enable_sma20_gt_sma50 = evt.checked(),
                }
                "SMA20 > SMA50"
            }
            label {
                input {
                    r#type: "checkbox",
                    checked: config.read().phase1.enable_sma50_gt_sma200,
                    oninput: move |evt| config.write().phase1.enable_sma50_gt_sma200 = evt.checked(),
                }
                "SMA50 > SMA200"
            }
            label {
                input {
                    r#type: "checkbox",
                    checked: config.read().phase1.enable_sma200_slope,
                    oninput: move |evt| config.write().phase1.enable_sma200_slope = evt.checked(),
                }
                "SMA200 slope positive"
            }
            label {
                "SMA10 period "
                input {
                    r#type: "number",
                    value: "{config.read().phase1.sma10_period}",
                    oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase1.sma10_period = v },
                }
            }
            label {
                "SMA20 period "
                input {
                    r#type: "number",
                    value: "{config.read().phase1.sma20_period}",
                    oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase1.sma20_period = v },
                }
            }
            label {
                "SMA50 period "
                input {
                    r#type: "number",
                    value: "{config.read().phase1.sma50_period}",
                    oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase1.sma50_period = v },
                }
            }
            label {
                "SMA200 period "
                input {
                    r#type: "number",
                    value: "{config.read().phase1.sma200_period}",
                    oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase1.sma200_period = v },
                }
            }
            label {
                "Slope lookback bars "
                input {
                    r#type: "number",
                    value: "{config.read().phase1.slope_lookback_bars}",
                    oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase1.slope_lookback_bars = v },
                }
            }

            h3 { "Phase 2 (30-Minute)" }
            label {
                input {
                    r#type: "checkbox",
                    checked: config.read().phase2.enable_close_gt_sma7,
                    oninput: move |evt| config.write().phase2.enable_close_gt_sma7 = evt.checked(),
                }
                "Close > SMA7"
            }
            label {
                input {
                    r#type: "checkbox",
                    checked: config.read().phase2.enable_sma7_gt_sma17,
                    oninput: move |evt| config.write().phase2.enable_sma7_gt_sma17 = evt.checked(),
                }
                "SMA7 > SMA17"
            }
            label {
                input {
                    r#type: "checkbox",
                    checked: config.read().phase2.enable_sma17_gt_sma33,
                    oninput: move |evt| config.write().phase2.enable_sma17_gt_sma33 = evt.checked(),
                }
                "SMA17 > SMA33"
            }
            label {
                input {
                    r#type: "checkbox",
                    checked: config.read().phase2.enable_sma33_gt_sma65,
                    oninput: move |evt| config.write().phase2.enable_sma33_gt_sma65 = evt.checked(),
                }
                "SMA33 > SMA65"
            }
            label {
                input {
                    r#type: "checkbox",
                    checked: config.read().phase2.enable_sma65_slope,
                    oninput: move |evt| config.write().phase2.enable_sma65_slope = evt.checked(),
                }
                "SMA65 slope positive"
            }
            label {
                "SMA7 period "
                input {
                    r#type: "number",
                    value: "{config.read().phase2.sma7_period}",
                    oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase2.sma7_period = v },
                }
            }
            label {
                "SMA17 period "
                input {
                    r#type: "number",
                    value: "{config.read().phase2.sma17_period}",
                    oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase2.sma17_period = v },
                }
            }
            label {
                "SMA33 period "
                input {
                    r#type: "number",
                    value: "{config.read().phase2.sma33_period}",
                    oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase2.sma33_period = v },
                }
            }
            label {
                "SMA65 period "
                input {
                    r#type: "number",
                    value: "{config.read().phase2.sma65_period}",
                    oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase2.sma65_period = v },
                }
            }
            label {
                "Slope lookback bars "
                input {
                    r#type: "number",
                    value: "{config.read().phase2.slope_lookback_bars}",
                    oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase2.slope_lookback_bars = v },
                }
            }

            h3 { "Phase 3 (2-Minute + VWAP)" }
            label {
                input {
                    r#type: "checkbox",
                    checked: config.read().phase3.enable_close_gt_sma20,
                    oninput: move |evt| config.write().phase3.enable_close_gt_sma20 = evt.checked(),
                }
                "Close > SMA20"
            }
            label {
                input {
                    r#type: "checkbox",
                    checked: config.read().phase3.enable_sma20_gt_sma50,
                    oninput: move |evt| config.write().phase3.enable_sma20_gt_sma50 = evt.checked(),
                }
                "SMA20 > SMA50"
            }
            label {
                input {
                    r#type: "checkbox",
                    checked: config.read().phase3.enable_sma50_gt_sma100,
                    oninput: move |evt| config.write().phase3.enable_sma50_gt_sma100 = evt.checked(),
                }
                "SMA50 > SMA100"
            }
            label {
                input {
                    r#type: "checkbox",
                    checked: config.read().phase3.enable_vwap,
                    oninput: move |evt| config.write().phase3.enable_vwap = evt.checked(),
                }
                "Close > VWAP"
            }
            label {
                "SMA20 period "
                input {
                    r#type: "number",
                    value: "{config.read().phase3.sma20_period}",
                    oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase3.sma20_period = v },
                }
            }
            label {
                "SMA50 period "
                input {
                    r#type: "number",
                    value: "{config.read().phase3.sma50_period}",
                    oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase3.sma50_period = v },
                }
            }
            label {
                "SMA100 period "
                input {
                    r#type: "number",
                    value: "{config.read().phase3.sma100_period}",
                    oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase3.sma100_period = v },
                }
            }

            div {
                button {
                    onclick: move |_| config.set(FilterConfigDto::default()),
                    "Reset to defaults"
                }
                button {
                    onclick: move |_| {
                        let api = api.clone();
                        let token = state.read().token.clone().unwrap_or_default();
                        let payload = config.read().clone();
                        spawn(async move {
                            if api.put_filter_config(&token, &payload).await.is_ok() {
                                save_status.set("saved".to_string());
                            } else {
                                save_status.set("save failed".to_string());
                            }
                        });
                    },
                    "Save"
                }
                span { "{save_status}" }
            }
        }
    }
}
