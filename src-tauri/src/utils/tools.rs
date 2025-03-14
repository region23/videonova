use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri::Manager;

/// Получает путь к исполняемым файлам внешних инструментов
pub fn get_tool_path(app_handle: &AppHandle, tool_name: &str) -> PathBuf {
    #[cfg(target_os = "macos")]
    let suffix = "";
    #[cfg(target_os = "windows")]
    let suffix = ".exe";
    
    // В Tauri 2.0 используется метод app_data_dir() через путь app_handle.path()
    // Создаем путь к директории tools в app_data_dir
    match app_handle.path().app_data_dir() {
        Ok(app_dir) => {
            let tools_dir = app_dir.join("tools");
            tools_dir.join(format!("{}{}", tool_name, suffix))
        },
        Err(_) => {
            // Если не удалось получить app_data_dir, используем текущую директорию
            PathBuf::from("./tools").join(format!("{}{}", tool_name, suffix))
        }
    }
} 