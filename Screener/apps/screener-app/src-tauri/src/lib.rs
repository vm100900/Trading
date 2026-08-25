mod commands;
mod token_store;

use std::sync::Arc;

use token_store::{FileTokenStore, TokenStore};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            use tauri::Manager;
            let data_dir = app.path().app_data_dir().expect("app data dir should be resolvable");
            let store: Arc<dyn TokenStore> = Arc::new(FileTokenStore::new(data_dir.join("auth_token.txt")));
            app.manage(store);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, commands::get_auth_token, commands::set_auth_token])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
