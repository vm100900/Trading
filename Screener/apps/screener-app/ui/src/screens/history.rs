use dioxus::prelude::*;

use crate::api::ApiClient;
use crate::state::AppState;

#[component]
pub fn History() -> Element {
    let state = use_context::<Signal<AppState>>();
    let api = use_context::<ApiClient>();

    let runs = use_resource({
        let api = api.clone();
        move || {
            let api = api.clone();
            let token = state.read().token.clone().unwrap_or_default();
            async move { api.list_runs(&token).await.unwrap_or_default() }
        }
    });

    rsx! {
        section {
            h2 { "History" }
            match &*runs.read() {
                None => rsx! { p { "Loading..." } },
                Some(runs) if runs.is_empty() => rsx! { p { "No runs yet." } },
                Some(runs) => rsx! {
                    ul {
                        for run in runs.iter() {
                            li { "{run.started_at} — {run.status:?} — {run.final_watchlist.len()} symbols" }
                        }
                    }
                },
            }
        }
    }
}
