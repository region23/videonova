use tauri::{Window, Manager, Emitter};
use tokio::sync::mpsc;
use std::path::PathBuf;
use serde::Serialize;
use crate::services::transcription;

#[derive(Serialize)]
pub struct TranscriptionResult {
    pub vtt_path: String,
}

/// Transcribe audio file to VTT format using OpenAI Whisper API
#[tauri::command]
pub async fn transcribe_audio(
    audio_path: String,
    output_path: String,
    api_key: String,
    language: Option<String>,
    window: Window,
) -> Result<TranscriptionResult, String> {
    // Create progress channel
    let (tx, mut rx) = mpsc::channel::<transcription::TranscriptionProgress>(32);
    
    // Clone window handle for the progress monitoring task
    let window_clone = window.clone();

    // Spawn progress monitoring task
    let monitoring_task = tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            // Emit progress event to frontend
            if let Err(e) = window_clone.emit("transcription-progress", progress) {
                eprintln!("Failed to emit transcription progress: {}", e);
            }
        }
    });

    // Add a small delay before starting transcription,
    // to allow UI to update and video download to continue
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Start transcription
    let audio_file = PathBuf::from(audio_path);
    let output_dir = PathBuf::from(output_path);

    let result_path =
        transcription::transcribe_audio(
            &audio_file.to_string_lossy(),
            &output_dir.to_string_lossy(),
            &language.unwrap_or_else(|| "en".to_string()),
            &api_key,
            Some(tx)
        )
        .await
        .map_err(|e| e.to_string())?;

    // Wait for the monitoring task to complete
    let _ = monitoring_task.await;

    Ok(TranscriptionResult {
        vtt_path: result_path,
    })
}

/// Validate an OpenAI API key
#[tauri::command]
pub async fn validate_openai_key(api_key: String) -> Result<bool, String> {
    log::info!("Beginning OpenAI API key validation");

    // Create a client with detailed debug information
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("videonova-tts-client/1.0")
        .build()
        .unwrap_or_else(|e| {
            log::warn!("Could not create custom client, using default: {}", e);
            reqwest::Client::new()
        });

    log::info!("Sending test request to OpenAI API");
    
    let request_start = std::time::Instant::now();
    let response = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await;
    let request_duration = request_start.elapsed();
    
    log::info!("OpenAI API request took {} milliseconds", request_duration.as_millis());
    
    match response {
        Ok(resp) => {
            let status = resp.status();
            log::info!("OpenAI API response status: {}", status);
            
            if !status.is_success() {
                // Try to get detailed error info
                match resp.text().await {
                    Ok(text) => {
                        log::error!("OpenAI API error response: {}", text);
                    },
                    Err(e) => {
                        log::error!("Could not read OpenAI API error response: {}", e);
                    }
                }
            }
            
            Ok(status.is_success())
        },
        Err(e) => {
            log::error!("OpenAI API request failed: {}", e);
            
            // Additional network diagnostics
            if e.is_timeout() {
                log::error!("Request timed out - possible network issue");
            } else if e.is_connect() {
                log::error!("Connection error - possible firewall or proxy issue");
            } else if e.is_request() {
                log::error!("Request building error - possible TLS or library issue");
            }
            
            Err(e.to_string())
        }
    }
} 