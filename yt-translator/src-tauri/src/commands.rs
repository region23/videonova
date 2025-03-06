use reqwest;
use serde::Serialize;
use std::path::PathBuf;
use tauri::Emitter;
use tokio::sync::mpsc;

use crate::utils::youtube::{self, DownloadProgress, VideoInfo};

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
    let (tx, mut rx) = mpsc::channel(32);

    // Clone window handle for the progress monitoring task
    let progress_window = window.clone();

    // Spawn progress monitoring task
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            // Emit progress event to frontend
            if let Err(e) = progress_window.emit("download-progress", progress) {
                eprintln!("Failed to emit progress: {}", e);
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
