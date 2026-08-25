use std::sync::Arc;

use crate::token_store::TokenStore;

#[tauri::command]
pub fn get_auth_token(store: tauri::State<Arc<dyn TokenStore>>) -> Option<String> {
    store.get()
}

#[tauri::command]
pub fn set_auth_token(store: tauri::State<Arc<dyn TokenStore>>, token: String) {
    store.set(&token);
}
