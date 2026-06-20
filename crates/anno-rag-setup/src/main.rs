// Prevents an additional console window on Windows in release, DO NOT REMOVE!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::patch_claude_config,
            commands::download_models_progress,
            commands::init_vault_keyring,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
