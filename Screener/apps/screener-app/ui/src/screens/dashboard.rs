use dioxus::prelude::*;

use crate::api::ApiClient;
use crate::state::{AppState, Screen};
use crate::token_storage::{LocalTokenStorage, TokenStorage};

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
    let has_token = state.read().token.is_some();
    let mut token_input = use_signal(String::new);

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

            // Minimal auth: the server requires a bearer token and there is no
            // full login flow. Paste the token once; it's kept in the webview's
            // localStorage (LocalTokenStorage) and reloaded on next launch.
            div {
                h3 { "API token" }
                p {
                    if has_token { "A token is set." } else { "No token set — REST/WS calls will 401." }
                }
                input {
                    r#type: "password",
                    placeholder: "bearer token",
                    value: "{token_input}",
                    oninput: move |evt| token_input.set(evt.value()),
                }
                button {
                    onclick: move |_| {
                        let token = token_input.read().trim().to_string();
                        if !token.is_empty() {
                            LocalTokenStorage.set(&token);
                            state.write().token = Some(token);
                            token_input.set(String::new());
                        }
                    },
                    "Save token"
                }
            }
        }
    }
}
