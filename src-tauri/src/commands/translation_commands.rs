use tauri::{Window, Manager, Emitter};
use tokio::sync::mpsc;
use std::path::PathBuf;
use serde::Serialize;
use crate::services::translation;

#[derive(Serialize)]
pub struct TranslationResult {
    pub translated_vtt_path: String,
    pub base_filename: String,
}

/// Translate VTT file to target language using OpenAI GPT-4o-mini
#[tauri::command]
pub async fn translate_vtt(
    vtt_path: String,
    output_path: String,
    source_language: String,
    target_language: String,
    target_language_code: String,
    api_key: String,
    window: Window,
) -> Result<TranslationResult, String> {
    log::info!("Starting VTT translation to {}", target_language);
    
    let (tx, mut rx) = mpsc::channel::<translation::TranslationProgress>(32);
    
    // Clone window for progress updates
    let window_clone = window.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            window_clone.emit("translation-progress", progress).unwrap_or_else(|e| {
                eprintln!("Failed to emit progress: {}", e);
            });
        }
    });
    
    // Call the translation function
    let result_path = translation::translate_vtt(
        &vtt_path,
        &output_path,
        &source_language,
        &target_language,
        &target_language_code,
        &api_key,
        Some(tx),
    ).await.map_err(|e| e.to_string())?;
    
    // Extract base filename without extension
    let path = std::path::Path::new(&result_path);
    let base_filename = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    
    Ok(TranslationResult {
        translated_vtt_path: result_path,
        base_filename,
    })
} 