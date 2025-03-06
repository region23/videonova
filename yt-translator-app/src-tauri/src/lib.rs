use std::sync::Arc;

mod commands;
mod models;
mod utils;

use commands::TranslationState;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logger
    env_logger::init();

    tauri::Builder::default()
        .manage(Arc::new(TranslationState::default()))
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::initialize_app,
            commands::select_directory,
            commands::start_translation_process,
            commands::open_file,
        ])
        .setup(|_app| {
            // Initialize the application in background
            tauri::async_runtime::spawn(async {
                if let Err(e) = commands::initialize_app().await {
                    eprintln!("Failed to initialize app: {}", e);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
