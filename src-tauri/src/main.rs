// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use log::error;
use tauri::menu::{MenuBuilder, SubmenuBuilder};
use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;

mod commands;
mod utils;

fn main() {
    // Инициализируем логгер с тонкой настройкой
    utils::logger::init_logger();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
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
            let _store = app.store(".settings.dat")?;

            // Initialize tools in background
            tauri::async_runtime::spawn(async {
                if let Err(e) = utils::tools::init_tools(None).await {
                    error!("Failed to initialize tools: {}", e);
                }
            });
            
            // Проверка доступности сервисов при запуске приложения
            if let Some(main_window) = app.get_webview_window("main") {
                // Клонируем окно для использования в асинхронном контексте
                let window_clone = main_window.clone();
                
                tauri::async_runtime::spawn(async move {
                    // Небольшая задержка перед проверкой, чтобы приложение успело загрузиться
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    
                    // Проверяем доступность сервисов
                    match commands::check_services_availability(window_clone, None).await {
                        Ok(result) => {
                            if result.vpn_required {
                                log::warn!("VPN required: YouTube: {}, OpenAI: {}", 
                                          !result.youtube_blocked, 
                                          !result.openai_blocked);
                            } else {
                                log::info!("All services are available");
                            }
                        },
                        Err(e) => {
                            log::error!("Failed to check services availability: {}", e);
                        }
                    }
                });
            } else {
                log::error!("Main window not found");
            }

            Ok(())
        })
        .on_menu_event(|app_handle, event| {
            let window = app_handle.get_webview_window("main").unwrap();
            match event.id().0.as_str() {
                "settings" => {
                    // Emit event to show settings
                    window.emit("show-settings", ()).unwrap();
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_video_info,
            commands::download_video,
            commands::validate_openai_key,
            commands::transcribe_audio,
            commands::translate_vtt,
            commands::generate_speech,
            commands::process_video,
            commands::check_file_exists_command,
            commands::cleanup_temp_files,
            commands::open_file,
            commands::check_services_availability,
            commands::check_youtube_availability,
            commands::check_openai_availability,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
