use reqwest;
use serde::Serialize;
use std::path::PathBuf;
use tauri::Emitter;
use tokio::sync::mpsc;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use anyhow::anyhow;

use crate::utils::youtube::{self, DownloadProgress, VideoInfo};
use crate::utils::transcribe;
use crate::utils::common::sanitize_filename;

#[derive(Clone, Serialize)]
pub struct DownloadState {
    progress: DownloadProgress,
    #[serde(skip)]
    progress_sender: mpsc::Sender<DownloadProgress>,
}

#[derive(Serialize)]
pub struct DownloadResult {
    video_path: String,
    audio_path: String,
}

#[derive(Serialize)]
pub struct TranscriptionResult {
    vtt_path: String,
}

/// Get information about a YouTube video
#[tauri::command]
pub async fn get_video_info(url: String) -> Result<VideoInfo, String> {
    youtube::get_video_info(&url)
        .await
        .map_err(|e| e.to_string())
}

/// Start downloading a YouTube video
#[tauri::command]
pub async fn download_video(
    url: String,
    output_path: String,
    window: tauri::Window,
) -> Result<DownloadResult, String> {
    // Create progress channel
    let (tx, mut rx) = mpsc::channel::<DownloadProgress>(32);

    // Clone window handle for the progress monitoring task
    let progress_window = window.clone();
    
    // Track if audio file is completed
    let audio_completed = Arc::new(AtomicBool::new(false));
    let audio_completed_clone = audio_completed.clone();
    
    // Клонируем переменные для использования в замыкании
    let url_clone = url.clone();
    let output_path_clone = output_path.clone();

    // Spawn progress monitoring task
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            // Emit progress event to frontend
            if let Err(e) = progress_window.emit("download-progress", progress.clone()) {
                eprintln!("Failed to emit progress: {}", e);
            }
            
            // Check if audio download is complete
            if progress.component == "audio" && progress.progress >= 99.0 && 
               !audio_completed_clone.load(Ordering::Relaxed) {
                
                // Mark as completed to avoid duplicate events
                audio_completed_clone.store(true, Ordering::Relaxed);
                
                // Используем отдельную задачу для обработки события audio-ready,
                // чтобы не блокировать мониторинг загрузки видео
                let event_window = progress_window.clone();
                let url_event = url_clone.clone();
                let output_path_event = output_path_clone.clone();
                
                tokio::spawn(async move {
                    // Добавляем небольшую задержку для стабильности
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    
                    // Для события audio-ready нам нужен только путь к аудиофайлу
                    if let Ok(info) = youtube::get_video_info(&url_event).await {
                        let output_dir = PathBuf::from(&output_path_event);
                        let safe_title = sanitize_filename(&info.title);
                        let audio_path = output_dir.join(format!("{}_audio.m4a", safe_title));
                        
                        // Emit audio ready event with path
                        if let Err(e) = event_window.emit("audio-ready", audio_path.to_string_lossy().to_string()) {
                            eprintln!("Failed to emit audio-ready event: {}", e);
                        }
                    }
                });
            }
        }
    });

    // Start download
    let output_dir = PathBuf::from(output_path);
    let result = youtube::download_video(&url, &output_dir, Some(tx))
        .await
        .map_err(|e| e.to_string())?;

    Ok(DownloadResult {
        video_path: result.video_path.to_string_lossy().to_string(),
        audio_path: result.audio_path.to_string_lossy().to_string(),
    })
}

/// Transcribe audio file to VTT format using OpenAI Whisper API
#[tauri::command]
pub async fn transcribe_audio(
    audio_path: String,
    output_path: String,
    api_key: String,
    language: Option<String>,
    window: tauri::Window,
) -> Result<TranscriptionResult, String> {
    // Create progress channel
    let (tx, mut rx) = mpsc::channel::<transcribe::TranscriptionProgress>(32);

    // Clone window handle for the progress monitoring task
    let progress_window = window.clone();

    // Spawn progress monitoring task
    let monitoring_task = tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            // Emit progress event to frontend
            if let Err(e) = progress_window.emit("transcription-progress", progress) {
                eprintln!("Failed to emit transcription progress: {}", e);
            }
        }
    });

    // Добавляем небольшую задержку перед началом транскрибации,
    // чтобы дать возможность UI обновиться и загрузке видео продолжиться
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Start transcription
    let audio_file = PathBuf::from(audio_path);
    let output_dir = PathBuf::from(output_path);
    
    let result_path = transcribe::transcribe_audio(
        &audio_file,
        &output_dir,
        &api_key,
        language,
        Some(tx),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Подождем завершения задачи мониторинга (она должна завершиться 
    // после закрытия канала tx при завершении transcribe_audio)
    let _ = monitoring_task.await;

    Ok(TranscriptionResult {
        vtt_path: result_path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn validate_openai_key(api_key: String) -> Result<bool, String> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    Ok(response.status().is_success())
}
