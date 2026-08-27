use futures_util::StreamExt;
use gloo_net::websocket::futures::WebSocket;
use gloo_net::websocket::Message;

use crate::dto::ProgressEvent;

pub fn build_stream_url(http_base: &str, token: &str, run_id: &str) -> String {
    let ws_base = if let Some(rest) = http_base.strip_prefix("https") {
        format!("wss{rest}")
    } else if let Some(rest) = http_base.strip_prefix("http") {
        format!("ws{rest}")
    } else {
        http_base.to_string()
    };
    format!("{ws_base}/runs/{run_id}/stream?token={}", urlencoding::encode(token))
}

pub struct StreamHandlers {
    pub on_event: Box<dyn FnMut(ProgressEvent)>,
    pub on_close: Box<dyn FnMut()>,
}

/// Opens the run-progress WebSocket and forwards parsed `ProgressEvent`s to
/// `handlers.on_event`, calling `handlers.on_close` when the socket closes.
/// Needs a running browser/wasm environment (`gloo_net` wraps the browser
/// `WebSocket` API) — like `LocalTokenStorage`, this is compile-verified
/// only in this environment, not behavior-tested.
pub fn connect_run_stream(http_base: &str, token: &str, run_id: &str, handlers: StreamHandlers) {
    let url = build_stream_url(http_base, token, run_id);
    wasm_bindgen_futures::spawn_local(async move {
        let StreamHandlers { mut on_event, mut on_close } = handlers;
        let ws = match WebSocket::open(&url) {
            Ok(ws) => ws,
            Err(_) => {
                on_close();
                return;
            }
        };
        let (_write, mut read) = ws.split();
        while let Some(Ok(Message::Text(text))) = read.next().await {
            if let Ok(event) = serde_json::from_str::<ProgressEvent>(&text) {
                on_event(event);
            }
        }
        on_close();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_url_with_the_token_as_a_query_param_not_a_header() {
        let url = build_stream_url("http://test.local:8080", "secret-token", "run-1");
        assert_eq!(url, "ws://test.local:8080/runs/run-1/stream?token=secret-token");
    }

    #[test]
    fn upgrades_https_to_wss() {
        let url = build_stream_url("https://test.local:8080", "secret-token", "run-1");
        assert_eq!(url, "wss://test.local:8080/runs/run-1/stream?token=secret-token");
    }

    #[test]
    fn percent_encodes_special_characters_in_the_token() {
        let url = build_stream_url("http://test.local:8080", "a+b/c=", "run-1");
        assert_eq!(url, "ws://test.local:8080/runs/run-1/stream?token=a%2Bb%2Fc%3D");
    }
}
