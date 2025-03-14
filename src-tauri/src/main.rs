// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use log::error;
use tauri::Manager;
use tauri::Emitter;
use tauri_plugin_store::StoreExt;
use tauri::menu::{MenuBuilder, SubmenuBuilder};
use tauri_plugin_clipboard_manager;
use tauri_plugin_dialog;
use tauri_plugin_opener;
use tauri_plugin_shell;
use tauri_plugin_store;

// Модули приложения
mod commands;
mod utils;
mod config;
mod services;
mod models;
#[path = "errors/mod.rs"]
mod errors;
mod events;

// Импорты для команд
use crate::commands::*;

fn main() {
    // Инициализируем логгер
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Create app submenu
            let app_menu = SubmenuBuilder::new(app, "App")
                .text("about", "About Videonova")
                .separator()
                .text("settings", "Settings")
                .separator()
                .quit()
                .build()?;

            let edit_menu = SubmenuBuilder::new(app, "Edit")
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;
                
            // Create main menu
            let menu = MenuBuilder::new(app).items(&[&app_menu, &edit_menu]).build()?;

            app.set_menu(menu)?;

            // Initialize store
            let _store = app.get_store(".settings.dat").ok_or_else(|| {
                error!("Failed to initialize store");
                Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Failed to initialize store"))
            })?;

            if let Some(main_window) = app.get_webview_window("main") {
                let _ = main_window.show();
            }

            Ok(())
        })
        .on_menu_event(|app_handle, event| {
            if let Some(window) = app_handle.get_webview_window("main") {
                match event.id().0.as_str() {
                    "settings" => {
                        // Emit event to show settings
                        let _ = window.emit("show-settings", ());
                    },
                    "quit" => {
                        std::process::exit(0);
                    },
                    _ => {}
                }
            }
        })
        .on_window_event(|_app_handle, event| {
            match event {
                tauri::WindowEvent::Destroyed { .. } => {
                    std::process::exit(0);
                },
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            // TTS commands
            get_tts_config,
            save_tts_config,
            get_tts_engines,
            get_default_tts_engine,
            // Используем полное имя вместо двух одинаковых имен команд
            tts_commands::generate_speech,
            // Video commands
            get_video_info,
            download_video,
            // Transcription commands
            transcribe_audio,
            validate_openai_key,
            // Translation commands
            translate_vtt,
            // Utility commands
            check_file_exists_command,
            cleanup_temp_files,
            check_services_availability,
            check_fish_speech_installed,
            // Speech commands
            speech_commands::generate_speech_v2,
            speech_commands::process_video,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app_handle, event| {
            match event {
                // Add event handling if needed
                _ => {}
            }
        });
}
