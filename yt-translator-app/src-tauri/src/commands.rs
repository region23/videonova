use crate::models::{ProcessStatus, ProgressUpdate, TranslationRequest};
use crate::utils::{init_tools, Translator};
use tauri::{AppHandle, State, Manager};
use log::{debug, error, info};
use std::sync::Arc;
use std::process::Command;
use tokio::sync::{mpsc, Mutex};
use dirs;

/// State for managing the translation process
pub struct TranslationState {
    /// Channel for receiving progress updates
    progress_receiver: Mutex<Option<mpsc::Receiver<ProgressUpdate>>>,
}

impl Default for TranslationState {
    fn default() -> Self {
        Self {
            progress_receiver: Mutex::new(None),
        }
    }
}

/// Initialize the application
#[tauri::command]
pub async fn initialize_app() -> Result<(), String> {
    // Initialize external tools
    init_tools(None)
        .await
        .map_err(|e| format!("Failed to initialize tools: {}", e))?;

    Ok(())
}

/// Select a directory for output
#[tauri::command]
pub async fn select_directory(_app_handle: tauri::AppHandle) -> Result<String, String> {
    // Используем временное решение - возвращаем директорию для документов пользователя
    let docs_dir = dirs::document_dir()
        .or_else(|| dirs::home_dir())
        .ok_or_else(|| "Could not find documents or home directory".to_string())?;
    
    Ok(docs_dir.to_string_lossy().to_string())
}

/// Start the translation process
#[tauri::command]
pub async fn start_translation_process(
    app_handle: AppHandle,
    translation_state: State<'_, Arc<TranslationState>>,
    request: TranslationRequest,
) -> Result<(), String> {
    info!("Starting translation process for URL: {}", request.youtube_url);

    // Create channels for progress updates
    let (progress_sender, progress_receiver) = mpsc::channel::<ProgressUpdate>(100);

    // Store the receiver in the state
    {
        let mut receiver_guard = translation_state.progress_receiver.lock().await;
        *receiver_guard = Some(progress_receiver);
    }

    // Create translator
    let translator = Translator::new(request, progress_sender.clone())
        .map_err(|e| format!("Failed to create translator: {}", e))?;

    // Clone app handle for the task
    let app_handle_clone = app_handle.clone();
    let state_clone = translation_state.inner().clone();

    // Spawn a task to handle progress updates
    tokio::spawn(async move {
        let mut receiver_guard = state_clone.progress_receiver.lock().await;
        if let Some(receiver) = &mut *receiver_guard {
            while let Some(update) = receiver.recv().await {
                debug!("Progress update: {:?}", update);
                
                // Emit progress event to frontend
                if let Err(e) = app_handle_clone.emit("translation_progress", update.clone()) {
                    error!("Failed to emit progress event: {}", e);
                }
                
                // If completed or error, break the loop
                if update.status == ProcessStatus::Completed || update.status == ProcessStatus::Error {
                    break;
                }
            }
        }
    });

    // Spawn a task to run the translation process
    tokio::spawn(async move {
        match translator.start().await {
            Ok(output_path) => {
                info!("Translation completed successfully: {}", output_path);
            }
            Err(e) => {
                error!("Translation failed: {}", e);
                
                // Send error status
                if let Err(send_err) = progress_sender
                    .send(ProgressUpdate {
                        status: ProcessStatus::Error,
                        progress: 0.0,
                        message: Some(e.to_string()),
                        output_file: None,
                    })
                    .await
                {
                    error!("Failed to send error status: {}", send_err);
                }
            }
        }
    });

    Ok(())
}

/// Open a file with the default application
#[tauri::command]
pub async fn open_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/c", &path])
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }
    
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }
    
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }
    
    Ok(())
} 