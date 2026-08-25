use std::sync::Mutex;

pub trait TokenStore: Send + Sync {
    fn get(&self) -> Option<String>;
    fn set(&self, token: &str);
}

#[derive(Default)]
pub struct InMemoryTokenStore {
    token: Mutex<Option<String>>,
}

impl InMemoryTokenStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenStore for InMemoryTokenStore {
    fn get(&self) -> Option<String> {
        self.token.lock().expect("token store mutex poisoned").clone()
    }

    fn set(&self, token: &str) {
        *self.token.lock().expect("token store mutex poisoned") = Some(token.to_string());
    }
}

/// Persists the token to a plain file in the app's data directory.
/// Constructing this needs a running Tauri app context (to resolve the
/// data directory via `tauri::Manager::path()`), so — like `IbkrClient` in
/// screener-core and `RealScreeningEngine` in screener-service — this is
/// compile-verified only in this environment, not behavior-tested.
pub struct FileTokenStore {
    path: std::path::PathBuf,
}

impl FileTokenStore {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self { path }
    }
}

impl TokenStore for FileTokenStore {
    fn get(&self) -> Option<String> {
        std::fs::read_to_string(&self.path).ok().map(|s| s.trim().to_string())
    }

    fn set(&self, token: &str) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&self.path, token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_before_anything_is_set() {
        let store = InMemoryTokenStore::new();
        assert_eq!(store.get(), None);
    }

    #[test]
    fn returns_the_token_after_set() {
        let store = InMemoryTokenStore::new();
        store.set("abc123");
        assert_eq!(store.get(), Some("abc123".to_string()));
    }

    #[test]
    fn set_overwrites_the_previous_token() {
        let store = InMemoryTokenStore::new();
        store.set("first");
        store.set("second");
        assert_eq!(store.get(), Some("second".to_string()));
    }
}
