use dioxus::prelude::*;

mod api;
mod dto;
mod screens;
mod state;
mod token_storage;
mod ws;

use api::ApiClient;
use screens::dashboard::Dashboard;
use screens::filter_config::FilterConfig;
use screens::history::History;
use screens::live_run::LiveRun;
use screens::watchlist::Watchlist;
use state::{AppState, Screen};
use token_storage::{LocalTokenStorage, TokenStorage};

const API_BASE_URL: &str = "http://10.0.2.2:8080";

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut state = use_context_provider(|| Signal::new(AppState::default()));
    let api = use_context_provider(|| ApiClient::new(API_BASE_URL));

    // Two separate effects, deliberately: `Signal<AppState>` is reactive at
    // whole-struct granularity, so an effect that both reads and writes it
    // would retrigger itself on every write. This one only ever writes
    // `state` and never reads it, so it runs once on mount and never
    // retriggers from its own write.
    use_effect(move || {
        if let Some(token) = LocalTokenStorage.get() {
            state.write().token = Some(token);
        }
    });

    // This one reads `state` (so it reruns whenever any field of it
    // changes, including its own write below) but guards the write with a
    // check against the value it's about to set — after the first run
    // sets `screen`, the guard makes every subsequent rerun a no-op, so
    // the self-triggering chain converges instead of looping forever.
    use_effect(move || {
        let Some(run_id) = state.read().active_run_id.clone() else { return };
        if state.read().screen == Screen::LiveRun {
            return;
        }
        let token = state.read().token.clone().unwrap_or_default();
        let api = api.clone();
        spawn(async move {
            if let Ok(record) = api.get_run(&token, &run_id).await {
                if record.status == dto::RunStatus::Running {
                    state.write().screen = Screen::LiveRun;
                }
            }
        });
    });

    rsx! {
        nav {
            button { onclick: move |_| state.write().screen = Screen::Dashboard, "dashboard" }
            button { onclick: move |_| state.write().screen = Screen::FilterConfig, "filter-config" }
            button { onclick: move |_| state.write().screen = Screen::LiveRun, "live-run" }
            button { onclick: move |_| state.write().screen = Screen::Watchlist, "watchlist" }
            button { onclick: move |_| state.write().screen = Screen::History, "history" }
        }
        main {
            match state.read().screen {
                Screen::Dashboard => rsx! { Dashboard {} },
                Screen::FilterConfig => rsx! { FilterConfig {} },
                Screen::LiveRun => rsx! { LiveRun {} },
                Screen::Watchlist => rsx! { Watchlist {} },
                Screen::History => rsx! { History {} },
            }
        }
    }
}
