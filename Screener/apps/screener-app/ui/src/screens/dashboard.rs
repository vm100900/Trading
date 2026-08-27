use dioxus::prelude::*;

use crate::api::ApiClient;
use crate::state::{AppState, Screen};

#[component]
pub fn Dashboard() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let api = use_context::<ApiClient>();

    let health = use_resource({
        let api = api.clone();
        move || {
            let api = api.clone();
            async move { api.get_health().await.is_ok() }
        }
    });

    let gateway_text = match &*health.read() {
        Some(true) => "reachable",
        Some(false) => "unreachable",
        None => "checking...",
    };

    let last_run_text = match &state.read().last_run {
        Some(r) => format!("{} ({} symbols)", r.status, r.final_watchlist.len()),
        None => "none yet".to_string(),
    };

    let run_disabled = state.read().active_run_id.is_some();

    rsx! {
        section {
            h2 { "Dashboard" }
            p { "Gateway: {gateway_text}" }
            p { "Last run: {last_run_text}" }
            button {
                disabled: run_disabled,
                onclick: move |_| {
                    let api = api.clone();
                    spawn(async move {
                        let token = state.read().token.clone().unwrap_or_default();
                        if let Ok(record) = api.trigger_run(&token).await {
                            state.write().active_run_id = Some(record.id);
                            state.write().screen = Screen::LiveRun;
                        }
                    });
                },
                "Run Screener"
            }
        }
    }
}
