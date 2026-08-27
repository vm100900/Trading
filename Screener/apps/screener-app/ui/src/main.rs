use dioxus::prelude::*;

mod dto;

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
