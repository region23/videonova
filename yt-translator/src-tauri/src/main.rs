// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;
use log::error;

mod utils;
mod commands;

fn main() {
    // Initialize logger
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            
            // Initialize tools in background
            tauri::async_runtime::spawn(async {
                if let Err(e) = utils::tools::init_tools(None).await {
                    error!("Failed to initialize tools: {}", e);
                }
            });
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_video_info,
            commands::download_video,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
