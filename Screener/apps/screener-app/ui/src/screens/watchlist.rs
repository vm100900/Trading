use dioxus::prelude::*;

use crate::state::AppState;

#[component]
pub fn Watchlist() -> Element {
    let state = use_context::<Signal<AppState>>();
    let symbols = state.read().last_run.as_ref().map(|r| r.final_watchlist.clone()).unwrap_or_default();

    rsx! {
        section {
            h2 { "Watchlist" }
            if symbols.is_empty() {
                p { "No results yet." }
            } else {
                ul {
                    for symbol in symbols {
                        li { "{symbol}" }
                    }
                }
            }
        }
    }
}
