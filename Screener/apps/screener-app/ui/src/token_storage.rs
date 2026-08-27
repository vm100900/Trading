use std::sync::Mutex;

use gloo_storage::{LocalStorage, Storage};

pub trait TokenStorage: Send + Sync {
    fn get(&self) -> Option<String>;
    fn set(&self, token: &str);
}

#[derive(Default)]
pub struct InMemoryTokenStorage {
    token: Mutex<Option<String>>,
}

impl InMemoryTokenStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenStorage for InMemoryTokenStorage {
    fn get(&self) -> Option<String> {
        self.token.lock().expect("token storage mutex poisoned").clone()
    }

    fn set(&self, token: &str) {
        *self.token.lock().expect("token storage mutex poisoned") = Some(token.to_string());
    }
}

const STORAGE_KEY: &str = "screener_auth_token";

/// Persists the token in the webview's browser `localStorage`. Needs a
/// running wasm/browser environment (`web_sys::window()` must resolve), so
/// — like `FileTokenStore` in this app's JS predecessor — this is
/// compile-verified only in this environment, not behavior-tested.
pub struct LocalTokenStorage;

impl TokenStorage for LocalTokenStorage {
    fn get(&self) -> Option<String> {
        LocalStorage::get(STORAGE_KEY).ok()
    }

    fn set(&self, token: &str) {
        let _ = LocalStorage::set(STORAGE_KEY, token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_before_anything_is_set() {
        let store = InMemoryTokenStorage::new();
        assert_eq!(store.get(), None);
    }

    #[test]
    fn returns_the_token_after_set() {
        let store = InMemoryTokenStorage::new();
        store.set("abc123");
        assert_eq!(store.get(), Some("abc123".to_string()));
    }

    #[test]
    fn set_overwrites_the_previous_token() {
        let store = InMemoryTokenStorage::new();
        store.set("first");
        store.set("second");
        assert_eq!(store.get(), Some("second".to_string()));
    }
}
