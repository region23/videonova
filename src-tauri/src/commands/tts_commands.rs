use tauri::{Window, State, Emitter};
use tauri_plugin_store::Store;
use serde_json::{Value, json};
use log::error;

use crate::services::tts as tts_service;
use crate::models::{SpeechGenerationRequest, SpeechGenerationResult};
use crate::errors::AppResult;
use crate::config::AppConfig;
use crate::config::tts;

/// Get the current TTS configuration
#[tauri::command]
pub async fn get_tts_config(store: State<'_, Store<tauri::Wry>>) -> AppResult<Value> {
    let config = tts::load_config(store.inner()).await?;
    Ok(json!(config))
}

/// Save TTS configuration
#[tauri::command]
pub async fn save_tts_config(store: State<'_, Store<tauri::Wry>>, config: Value) -> AppResult<()> {
    let tts_config = serde_json::from_value(config)?;
    tts::save_config(store.inner(), tts_config).await?;
    Ok(())
}

/// Get available TTS engines
#[tauri::command]
pub async fn get_tts_engines() -> AppResult<Vec<String>> {
    Ok(tts_service::get_available_engines())
}

/// Get default TTS engine
#[tauri::command]
pub async fn get_default_tts_engine(config: State<'_, AppConfig>) -> AppResult<String> {
    Ok(tts_service::get_default_engine(&config))
}

#[tauri::command]
pub async fn generate_speech(
    window: Window,
    config: State<'_, AppConfig>,
    request: SpeechGenerationRequest,
) -> AppResult<SpeechGenerationResult> {
    let service = tts_service::get_tts_service(&request.engine, &config)?;
    let audio_config = tts_service::AudioProcessingConfig::default();
    
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(32);
    
    let window_clone = window.clone();
    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            window_clone.emit("tts-progress", progress).unwrap_or_else(|e| {
                error!("Failed to emit progress: {}", e);
            });
        }
    });
    
    service.generate_speech(&request, &audio_config, Some(progress_tx)).await
} 