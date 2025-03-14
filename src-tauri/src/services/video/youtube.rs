use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Child};
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::task;
use tokio::time::timeout;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_store::StoreExt;
use tauri::Window;
use crate::errors::AppResult;
use crate::utils::tools::get_tool_path;
use crate::utils::common::{sanitize_filename, check_file_exists_and_valid};
use tauri::AppHandle;
use crate::commands::video_commands::DownloadResponse;

// Structure for storing YouTube cookies
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct YoutubeCookies {
    pub browser: String,
    pub last_used: String, // ISO timestamp
    pub valid: bool,
}

// Cookie manager for YouTube
pub struct YoutubeCookieManager {}

impl YoutubeCookieManager {
    // Save cookies to the store
    pub async fn save_cookies(app_handle: &tauri::AppHandle, browser: &str, valid: bool) -> Result<()> {
        info!("Saving YouTube cookies from browser: {}", browser);
        let store = app_handle.store(".settings.dat")?;
        
        // Get current time as ISO string
        let now = std::time::SystemTime::now();
        let datetime = now.duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| anyhow!("Failed to get system time: {}", e))?;
        let timestamp = format!("{}", datetime.as_secs());
        
        let cookies = YoutubeCookies {
            browser: browser.to_string(),
            last_used: timestamp,
            valid,
        };
        
        // Convert to JSON value
        let json_value = serde_json::to_value(&cookies)
            .map_err(|e| anyhow!("Failed to serialize YouTube cookies: {}", e))?;
        
        store.set("youtube-cookies", json_value);
        
        store.save()
            .map_err(|e| anyhow!("Failed to persist YouTube cookies: {}", e))?;
        
        debug!("YouTube cookies saved successfully");
        Ok(())
    }
    
    // Load cookies from the store
    pub async fn load_cookies(app_handle: &tauri::AppHandle) -> Result<Option<YoutubeCookies>> {
        debug!("Loading YouTube cookies from store");
        let store = app_handle.store(".settings.dat")?;
        
        // Get the value as a JSON value from the store
        let json_value = store.get("youtube-cookies");
        
        // Convert from JSON value to our type if it exists
        let cookies = match json_value {
            Some(value) => {
                match serde_json::from_value::<YoutubeCookies>(value) {
                    Ok(cookies) => Some(cookies),
                    Err(e) => {
                        error!("Failed to deserialize YouTube cookies: {}", e);
                        None
                    }
                }
            },
            None => None,
        };
        
        if let Some(cookies) = &cookies {
            debug!("Found cookies from browser: {}, last used: {}", 
                   cookies.browser, cookies.last_used);
        } else {
            debug!("No saved YouTube cookies found");
        }
        
        Ok(cookies)
    }
    
    // Mark cookies as invalid
    pub async fn invalidate_cookies(app_handle: &tauri::AppHandle) -> Result<()> {
        debug!("Invalidating YouTube cookies");
        if let Ok(Some(mut cookies)) = Self::load_cookies(app_handle).await {
            cookies.valid = false;
            
            let store = app_handle.store(".settings.dat")?;
            
            // Convert to JSON value
            let json_value = serde_json::to_value(&cookies)
                .map_err(|e| anyhow!("Failed to serialize YouTube cookies: {}", e))?;
            
            store.set("youtube-cookies", json_value);
            
            store.save()
                .map_err(|e| anyhow!("Failed to persist YouTube cookie changes: {}", e))?;
            
            debug!("YouTube cookies marked as invalid");
        }
        
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub title: String,
    pub author: String,
    pub length_seconds: u64,
    pub thumbnail_url: String,
    pub video_url: String,
}

/// Структура для отслеживания прогресса скачивания
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub percent: f32,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed: String,
    pub eta: String,
    pub step: String,
}

/// Структура для результата скачивания
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadResult {
    pub video_path: PathBuf,
    pub audio_path: PathBuf,
    pub thumbnail_path: PathBuf,
    pub title: String,
    pub duration: f64,
}

impl DownloadResult {
    /// Converts paths to string representation for frontend
    pub fn to_frontend_response(&self) -> DownloadResponse {
        DownloadResponse {
            video_path: self.video_path.to_string_lossy().to_string(),
            audio_path: self.audio_path.to_string_lossy().to_string(),
        }
    }
}

/// Скачивает только аудио из YouTube
async fn download_audio(
    ytdlp_path: &str,
    url: &str,
    output_template: &str,
    progress_sender: Option<mpsc::Sender<DownloadProgress>>,
    cancellation_token: CancellationToken,
    child_processes: Arc<Mutex<Vec<Child>>>,
) -> AppResult<PathBuf> {
    info!("Downloading audio for URL: {}", url);
    
    // Заглушка для функции
    Ok(PathBuf::from(output_template))
}

/// Скачивает только видео из YouTube
async fn download_video_only(
    ytdlp_path: &str,
    url: &str,
    output_template: &str,
    progress_sender: Option<mpsc::Sender<DownloadProgress>>,
    cancellation_token: CancellationToken,
    child_processes: Arc<Mutex<Vec<Child>>>,
) -> AppResult<PathBuf> {
    info!("Downloading video for URL: {}", url);
    
    // Заглушка для функции
    Ok(PathBuf::from(output_template))
}

/// Получает информацию о видео по URL
pub async fn get_video_info(url: &str, _window: &Window) -> AppResult<VideoInfo> {
    // Заглушка для функции
    Ok(VideoInfo {
        title: "Sample Video".to_string(),
        author: "Sample Author".to_string(),
        length_seconds: 60,
        thumbnail_url: "https://example.com/thumbnail.jpg".to_string(),
        video_url: url.to_string(),
    })
}

/// Скачивает видео с YouTube
pub async fn download_video(
    app_handle: AppHandle,
    url: &str, 
    output_dir: &PathBuf, 
    window: Window
) -> AppResult<DownloadResult> {
    info!("Starting video download process for URL: {}", url);
    debug!("Output directory: {}", output_dir.display());
    
    // Create output directory if it doesn't exist
    if !output_dir.exists() {
        info!("Creating output directory: {}", output_dir.display());
        tokio::fs::create_dir_all(output_dir).await?;
    }
    
    // Get video info first to get the title
    info!("Fetching video information...");
    let video_info = get_video_info(url, &window).await?;
    let safe_title = sanitize_filename(&video_info.title);
    info!("Video title: {}", safe_title);

    // Check if files already exist
    let video_path = output_dir.join(format!("{}_video.mp4", safe_title));
    let audio_path = output_dir.join(format!("{}_audio.m4a", safe_title));

    if check_file_exists_and_valid(&video_path).await && check_file_exists_and_valid(&audio_path).await {
        info!("Found existing video and audio files, skipping download");
        return Ok(DownloadResult {
            video_path,
            audio_path,
            thumbnail_path: PathBuf::new(),
            title: safe_title,
            duration: 0.0,
        });
    }

    // Create cancellation token
    let cancellation_token = CancellationToken::new();
    let token_clone = cancellation_token.clone();

    // Setup Ctrl+C handler
    let ctrl_c_handler = tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            warn!("Received Ctrl+C signal, initiating graceful shutdown...");
            token_clone.cancel();
        }
    });

    // Get yt-dlp path
    let ytdlp_path = get_tool_path(&app_handle, "yt-dlp");
    debug!("Using yt-dlp from: {}", ytdlp_path.display());

    // Create output directory if it doesn't exist
    tokio::fs::create_dir_all(output_dir).await?;
    debug!("Ensured output directory exists");

    // Store child processes for cleanup
    let child_processes = Arc::new(Mutex::new(Vec::new()));
    let child_processes_clone = child_processes.clone();

    // Prepare output templates with yt-dlp's --restrict-filenames for consistency
    let audio_template = output_dir.join(format!("{}_audio.m4a", safe_title)).to_string_lossy().to_string();
    let video_template = output_dir.join(format!("{}_video.mp4", safe_title)).to_string_lossy().to_string();
    
    // Clone values for async tasks
    let url_clone = url.to_string();
    let ytdlp_path_clone = ytdlp_path.to_string_lossy().to_string();
    let cancellation_token_clone = cancellation_token.clone();
    let audio_template_clone = audio_template.clone();
    let video_template_clone = video_template.clone();

    // Create progress channels for audio and video
    let (audio_progress_tx, mut audio_progress_rx) = mpsc::channel(32);
    let (video_progress_tx, mut video_progress_rx) = mpsc::channel(32);

    // Start audio download task
    info!("Starting audio download task...");
    let child_processes_clone2 = child_processes_clone.clone();
    let audio_task = task::spawn(async move {
        download_audio(
            &ytdlp_path_clone,
            &url_clone,
            &audio_template_clone,
            Some(audio_progress_tx),
            cancellation_token_clone,
            child_processes_clone2,
        ).await
    });

    // Start video download task
    info!("Starting video download task...");
    let url_clone2 = url.to_string();
    let ytdlp_path_clone2 = ytdlp_path.to_string_lossy().to_string();
    let video_task = task::spawn(async move {
        download_video_only(
            &ytdlp_path_clone2,
            &url_clone2,
            &video_template_clone,
            Some(video_progress_tx),
            cancellation_token,
            child_processes,
        ).await
    });

    // Wait for both tasks to complete
    let (audio_result, video_result) = tokio::join!(audio_task, video_task);

    // Cancel Ctrl+C handler
    ctrl_c_handler.abort();

    // Check results
    let audio_path = audio_result.map_err(|e| anyhow!("Audio download task failed: {}", e))??;
    let video_path = video_result.map_err(|e| anyhow!("Video download task failed: {}", e))??;

    Ok(DownloadResult {
        video_path,
        audio_path,
        thumbnail_path: PathBuf::new(),
        title: safe_title,
        duration: 0.0,
    })
}

// ... rest of the file remains the same ... 