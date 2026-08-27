use std::collections::BTreeMap;

use dioxus::prelude::*;

use crate::dto::ProgressEvent;
use crate::state::{AppState, RunSummary, Screen};
use crate::ws::{connect_run_stream, StreamHandlers};

fn phase_label(phase: &str) -> &str {
    match phase {
        "Phase1" => "Daily",
        "Phase2" => "30-Minute",
        "Phase3" => "2-Minute",
        other => other,
    }
}

const API_BASE_URL: &str = match option_env!("SCREENER_API_BASE_URL") {
    Some(v) => v,
    None => "http://10.0.2.2:8080",
};

#[component]
pub fn LiveRun() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let mut rows: Signal<BTreeMap<String, String>> = use_signal(BTreeMap::new);
    // Guards against reopening the WebSocket on every unrelated write to
    // the shared `Signal<AppState>` (it's reactive at whole-struct
    // granularity, and this effect's own handlers write back into it) —
    // without this, any state change elsewhere in the app while this
    // screen is mounted would retrigger the effect and open a duplicate
    // connection.
    let mut connected = use_signal(|| false);

    let run_id = state.read().active_run_id.clone();

    use_effect(move || {
        if connected() {
            return;
        }
        let Some(run_id) = state.read().active_run_id.clone() else { return };
        connected.set(true);
        let token = state.read().token.clone().unwrap_or_default();

        connect_run_stream(
            API_BASE_URL,
            &token,
            &run_id,
            StreamHandlers {
                on_event: Box::new(move |event| match event {
                    ProgressEvent::Phase1 { total, started, passed, technical_failures, errors }
                    | ProgressEvent::Phase2 { total, started, passed, technical_failures, errors }
                    | ProgressEvent::Phase3 { total, started, passed, technical_failures, errors } => {
                        let phase = match &event {
                            ProgressEvent::Phase1 { .. } => "Phase1",
                            ProgressEvent::Phase2 { .. } => "Phase2",
                            _ => "Phase3",
                        };
                        rows.write().insert(
                            phase.to_string(),
                            format!(
                                "{}: {started}/{total} started, {passed} passed, {technical_failures} failed, {errors} errors",
                                phase_label(phase)
                            ),
                        );
                    }
                    ProgressEvent::Complete { final_watchlist } => {
                        state.write().active_run_id = None;
                        state.write().last_run = Some(RunSummary { status: "completed".to_string(), final_watchlist });
                        state.write().screen = Screen::Watchlist;
                    }
                    ProgressEvent::Failed { .. } => {
                        state.write().active_run_id = None;
                        state.write().last_run = Some(RunSummary { status: "failed".to_string(), final_watchlist: vec![] });
                    }
                }),
                on_close: Box::new(|| {}),
            },
        );
    });

    if run_id.is_none() {
        return rsx! { section { h2 { "Live Run" } p { "No run in progress." } } };
    }

    rsx! {
        section {
            h2 { "Live Run" }
            div {
                for (_, text) in rows.read().iter() {
                    p { "{text}" }
                }
            }
        }
    }
}
