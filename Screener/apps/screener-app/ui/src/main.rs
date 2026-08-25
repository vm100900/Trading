use dioxus::prelude::*;

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
