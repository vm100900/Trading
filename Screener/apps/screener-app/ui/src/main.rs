use dioxus::prelude::*;

mod api;
mod dto;
mod state;
mod token_storage;
mod ws;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        nav { "screener-app" }
        main { p { "loading..." } }
    }
}
