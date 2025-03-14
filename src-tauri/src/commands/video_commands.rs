use tauri::{Window, Manager, Emitter};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use crate::services::video::youtube;
use crate::utils::common;
use serde::{Serialize, Deserialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub progress: f32,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub status: String,
}

#[derive(Clone, Serialize)]
pub struct DownloadState {
    progress: DownloadProgress,
    #[serde(skip)]
    #[allow(dead_code)]
    progress_sender: mpsc::Sender<DownloadProgress>,
}

/// Response for the download video command
#[derive(Clone, Serialize, Deserialize)]
pub struct DownloadResponse {
    pub video_path: String,
    pub audio_path: String,
}

/// Get information about a YouTube video
#[tauri::command]
pub async fn get_video_info(window: Window, url: String) -> Result<crate::services::video::youtube::VideoInfo, String> {
    crate::services::video::youtube::get_video_info(&url, &window)
        .await
        .map_err(|e| e.to_string())
}

/// Start downloading a YouTube video
#[tauri::command]
pub async fn download_video(
    app_handle: tauri::AppHandle,
    url: String,
    output_dir: String,
    window: Window
) -> Result<DownloadResponse, String> {
    let (tx, mut rx) = mpsc::channel::<crate::services::video::youtube::DownloadProgress>(32);
    let output_dir = PathBuf::from(output_dir);
    let cancellation_token = CancellationToken::new();
    
    // Spawn task to handle progress updates
    let window_clone = window.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            if let Err(e) = window_clone.emit("download-progress", progress) {
                log::error!("Failed to emit progress: {}", e);
            }
        }
    });
    
    match youtube::download_video(app_handle, &url, &output_dir, window).await {
        Ok(result) => Ok(result.to_frontend_response()),
        Err(e) => Err(e.to_string()),
    }
} 