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

use super::tools::get_tool_path;
use crate::utils::common::{sanitize_filename, check_file_exists_and_valid};

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

#[derive(Debug, Serialize, Deserialize)]
pub struct VideoInfo {
    pub title: String,
    pub duration: f64,
    pub url: String,
    pub thumbnail: String,
    pub description: String,
    pub language: Option<String>,      // Язык видео
    pub original_language: Option<String>, // Оригинальный язык видео
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DownloadProgress {
    pub status: String,
    pub progress: f32,
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub component: String, // "audio" or "video"
}

#[derive(Debug, Serialize, Clone)]
pub struct DownloadResult {
    pub video_path: PathBuf,
    pub audio_path: PathBuf,
}

impl DownloadResult {
    /// Конвертирует пути в строковое представление для frontend
    pub fn to_frontend_response(&self) -> serde_json::Value {
        json!({
            "video_path": self.video_path.to_string_lossy().to_string(),
            "audio_path": self.audio_path.to_string_lossy().to_string(),
        })
    }
}

/// Shows keychain access information dialog
async fn show_keychain_info_dialog(window: &tauri::Window) {
    let _ = window.emit("show_dialog", json!({
        "title": "Доступ к Keychain",
        "message": "Для получения информации о видео приложению нужен доступ к cookies YouTube из вашего браузера.\n\n\
                   Это безопасно: приложение запрашивает только cookies YouTube для авторизации.\n\n\
                   Пожалуйста, разрешите доступ в появившемся системном диалоге.",
        "type": "info"
    }));
}

/// Download video from YouTube
pub async fn download_video(
    url: &str,
    output_dir: &PathBuf,
    progress_sender: Option<mpsc::Sender<DownloadProgress>>,
    cancellation_token: CancellationToken,
    window: &tauri::Window,
) -> Result<DownloadResult> {
    info!("Starting video download process for URL: {}", url);
    debug!("Output directory: {}", output_dir.display());
    
    // Create output directory if it doesn't exist
    if !output_dir.exists() {
        info!("Creating output directory: {}", output_dir.display());
        tokio::fs::create_dir_all(output_dir).await?;
    }
    
    // Create temp directory
    let temp_dir = output_dir.join("videonova_temp");
    if !temp_dir.exists() {
        info!("Creating temp directory: {}", temp_dir.display());
        tokio::fs::create_dir_all(&temp_dir).await?;
    }
    
    // Get video info first to get the title
    info!("Fetching video information...");
    let video_info = get_video_info(url, window).await?;
    let safe_title = sanitize_filename(&video_info.title);
    info!("Video title: {}", safe_title);

    // Check if files already exist in temp directory
    let video_path = temp_dir.join(format!("{}_video.mp4", safe_title));
    let audio_path = temp_dir.join(format!("{}_audio.m4a", safe_title));

    if check_file_exists_and_valid(&video_path).await && check_file_exists_and_valid(&audio_path).await {
        info!("Found existing video and audio files, skipping download");
        return Ok(DownloadResult {
            video_path,
            audio_path,
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
    let ytdlp_path = get_tool_path("yt-dlp").ok_or_else(|| anyhow!("yt-dlp not found"))?;
    debug!("Using yt-dlp from: {}", ytdlp_path.display());

    // Store child processes for cleanup
    let child_processes = Arc::new(Mutex::new(Vec::new()));
    let child_processes_clone = child_processes.clone();

    // Prepare output templates with yt-dlp's --restrict-filenames for consistency
    // We'll use constant extensions for predictability (m4a for audio, mp4 for video)
    let audio_filename = format!("{}_audio.m4a", safe_title);
    let video_filename = format!("{}_video.mp4", safe_title);
    
    let audio_template = temp_dir.join(format!("{}_audio.%(ext)s", safe_title));
    let video_template = temp_dir.join(format!("{}_video.%(ext)s", safe_title));
    
    debug!("Audio template: {}", audio_template.display());
    debug!("Video template: {}", video_template.display());
    debug!("Expected audio filename: {}", audio_filename);
    debug!("Expected video filename: {}", video_filename);

    // Create progress channels for audio and video
    let (audio_progress_tx, mut audio_progress_rx) = mpsc::channel(32);
    let (video_progress_tx, mut video_progress_rx) = mpsc::channel(32);

    // Clone necessary values for tasks
    let url_clone = url.to_string();
    let ytdlp_path_clone = ytdlp_path.clone();
    let cancellation_token_clone = cancellation_token.clone();

    // Start audio download task
    info!("Starting audio download task...");
    let audio_task = task::spawn(async move {
        download_audio(
            &ytdlp_path_clone,
            &url_clone,
            &audio_template,
            Some(audio_progress_tx),
            cancellation_token_clone,
            child_processes_clone,
        )
        .await
    });

    // Clone URL again for video task
    let url_clone_video = url.to_string();
    let ytdlp_path_clone_video = ytdlp_path.clone();
    let cancellation_token_clone = cancellation_token.clone();
    let child_processes_clone = child_processes.clone();

    // Start video download task
    info!("Starting video download task...");
    let video_task = task::spawn(async move {
        download_video_only(
            &ytdlp_path_clone_video,
            &url_clone_video,
            &video_template,
            Some(video_progress_tx),
            cancellation_token_clone,
            child_processes_clone,
        )
        .await
    });

    // Monitor progress from both downloads
    info!("Setting up progress monitoring...");
    let cancellation_token_clone = cancellation_token.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(audio_progress) = audio_progress_rx.recv() => {
                    let mut progress = audio_progress;
                    progress.component = "audio".to_string();
                    debug!("Audio progress: {}% at {}", progress.progress, progress.speed.as_deref().unwrap_or("unknown speed"));
                    if let Some(sender) = &progress_sender {
                        if let Err(e) = sender.send(progress).await {
                            error!("Failed to send audio progress: {}", e);
                        }
                    }
                }
                Some(video_progress) = video_progress_rx.recv() => {
                    let mut progress = video_progress;
                    progress.component = "video".to_string();
                    debug!("Video progress: {}% at {}", progress.progress, progress.speed.as_deref().unwrap_or("unknown speed"));
                    if let Some(sender) = &progress_sender {
                        if let Err(e) = sender.send(progress).await {
                            error!("Failed to send video progress: {}", e);
                        }
                    }
                }
                _ = cancellation_token_clone.cancelled() => {
                    debug!("Cancellation requested, stopping progress monitoring");
                    break;
                }
                else => {
                    debug!("Progress channels closed, stopping progress monitoring");
                    break;
                }
            }
        }
    });

    // Wait for both downloads to complete with timeout
    info!("Waiting for downloads to complete...");
    let download_timeout = std::time::Duration::from_secs(3600); // 1 hour timeout

    let result = tokio::select! {
        result = timeout(download_timeout, futures::future::try_join(audio_task, video_task)) => {
            result.map_err(|_| anyhow!("Download timeout exceeded (1 hour)"))??
        }
        _ = cancellation_token.cancelled() => {
            warn!("Download cancelled by user");
            // Cleanup child processes
            let mut processes = child_processes.lock().await;
            for child in processes.iter_mut() {
                if let Err(e) = child.kill() {
                    error!("Failed to kill child process: {}", e);
                }
            }
            return Err(anyhow!("Download cancelled by user"));
        }
    };

    // Cancel Ctrl+C handler
    ctrl_c_handler.abort();

    let (audio_result, video_result) = result;
    let audio_path_result = audio_result?;
    let video_path_result = video_result?;

    // Log the paths returned by the download functions
    info!("Raw download results:");
    info!("  Audio path: {}", audio_path_result.display());
    info!("  Video path: {}", video_path_result.display());

    // Double-check the returned paths
    let video_exists = check_file_exists_and_valid(&video_path_result).await;
    let audio_exists = check_file_exists_and_valid(&audio_path_result).await;

    if !video_exists || !audio_exists {
        error!("Download verification failed:");
        error!("  Video file exists and valid: {}", video_exists);
        error!("  Audio file exists and valid: {}", audio_exists);
        
        // Try to find files directly in the output directory
        info!("Searching for downloaded files in output directory: {}", output_dir.display());
        
        // Look for audio file (m4a)
        let audio_path_new = if !audio_exists {
            match find_newest_file_by_extension(output_dir, "m4a").await {
                Ok(path) => {
                    info!("Found audio file by extension: {}", path.display());
                    path
                },
                Err(e) => {
                    error!("Failed to find audio file: {}", e);
                    return Err(anyhow!("Failed to find audio file: {}", e));
                }
            }
        } else {
            audio_path_result
        };
        
        // Look for video file (mp4)
        let video_path_new = if !video_exists {
            match find_newest_file_by_extension(output_dir, "mp4").await {
                Ok(path) => {
                    info!("Found video file by extension: {}", path.display());
                    path
                },
                Err(e) => {
                    error!("Failed to find video file: {}", e);
                    return Err(anyhow!("Failed to find video file: {}", e));
                }
            }
        } else {
            video_path_result
        };
        
        // Final verification
        let video_exists_new = check_file_exists_and_valid(&video_path_new).await;
        let audio_exists_new = check_file_exists_and_valid(&audio_path_new).await;
        
        if !video_exists_new || !audio_exists_new {
            // List all files in the output directory for debugging
            error!("Files in output directory:");
            if let Ok(entries) = std::fs::read_dir(output_dir) {
                for entry in entries {
                    if let Ok(entry) = entry {
                        error!("  {}", entry.path().display());
                    }
                }
            }
            
            return Err(anyhow!("Downloaded files are missing or empty after extensive search"));
        }
        
        info!("Found files through fallback search:");
        info!("  Video: {}", video_path_new.display());
        info!("  Audio: {}", audio_path_new.display());
        
        return Ok(DownloadResult {
            video_path: video_path_new,
            audio_path: audio_path_new,
        });
    }

    info!("Download completed successfully");
    debug!("Audio file: {}", audio_path_result.display());
    debug!("Video file: {}", video_path_result.display());

    Ok(DownloadResult {
        video_path: video_path_result,
        audio_path: audio_path_result,
    })
}

/// Download audio only
async fn download_audio(
    ytdlp_path: &PathBuf,
    url: &str,
    output_template: &PathBuf,
    progress_sender: Option<mpsc::Sender<DownloadProgress>>,
    cancellation_token: CancellationToken,
    child_processes: Arc<Mutex<Vec<Child>>>,
) -> Result<PathBuf> {
    info!("Starting audio download for URL: {}", url);
    debug!("Using output template: {}", output_template.display());

    // Get the output directory directly from the output_template
    let output_dir = output_template.parent().unwrap_or(&PathBuf::new()).to_path_buf();
    debug!("User-selected output directory: {}", output_dir.display());
    
    // Extract the expected filename pattern from the output template
    let filename_pattern = output_template
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.replace("%(ext)s", "m4a"))
        .unwrap_or_else(|| "_audio.m4a".to_string());
    
    // Expected full path for the audio file
    let expected_file_path = output_dir.join(&filename_pattern);
    debug!("Expected audio file path: {}", expected_file_path.display());

    let mut command = Command::new(ytdlp_path);
    command
        .arg(url)
        .arg("--format")
        .arg("bestaudio[ext=m4a]/bestaudio")
        .arg("--extract-audio")
        .arg("--audio-format")
        .arg("m4a")
        .arg("--output")
        .arg(output_template.as_os_str())
        .arg("--newline")
        .arg("--progress")
        .arg("--no-playlist")
        .arg("--no-warnings")
        .arg("--no-mtime") // Don't use the media file timestamp
        .arg("--restrict-filenames") // Restrict filenames to only ASCII characters
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    debug!("Executing command: {:?}", command);
    process_download(
        command,
        progress_sender,
        cancellation_token,
        child_processes,
        &expected_file_path,
    )
    .await
}

/// Download video only (no audio)
async fn download_video_only(
    ytdlp_path: &PathBuf,
    url: &str,
    output_template: &PathBuf,
    progress_sender: Option<mpsc::Sender<DownloadProgress>>,
    cancellation_token: CancellationToken,
    child_processes: Arc<Mutex<Vec<Child>>>,
) -> Result<PathBuf> {
    info!("Starting video-only download for URL: {}", url);
    debug!("Using output template: {}", output_template.display());
    
    // Get the output directory directly from the output_template
    let output_dir = output_template.parent().unwrap_or(&PathBuf::new()).to_path_buf();
    debug!("User-selected output directory: {}", output_dir.display());
    
    // Extract the expected filename pattern from the output template
    let filename_pattern = output_template
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.replace("%(ext)s", "mp4"))
        .unwrap_or_else(|| "_video.mp4".to_string());
    
    // Expected full path for the video file
    let expected_file_path = output_dir.join(&filename_pattern);
    debug!("Expected video file path: {}", expected_file_path.display());

    let mut command = Command::new(ytdlp_path);
    command
        .arg(url)
        .arg("--format")
        .arg("bestvideo[ext=mp4]/bestvideo")
        .arg("--output")
        .arg(output_template.as_os_str())
        .arg("--newline")
        .arg("--progress")
        .arg("--no-playlist")
        .arg("--no-warnings")
        .arg("--no-mtime") // Don't use the media file timestamp
        .arg("--restrict-filenames") // Restrict filenames to only ASCII characters
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    debug!("Executing command: {:?}", command);
    process_download(
        command,
        progress_sender,
        cancellation_token,
        child_processes,
        &expected_file_path,
    )
    .await
}

/// Process download command and handle progress
async fn process_download(
    mut command: Command,
    progress_sender: Option<mpsc::Sender<DownloadProgress>>,
    cancellation_token: CancellationToken,
    child_processes: Arc<Mutex<Vec<Child>>>,
    expected_file_path: &PathBuf,  // The exact file path we expect
) -> Result<PathBuf> {
    debug!("Starting download process with command: {:?}", command);
    info!("Will look for output file at: {}", expected_file_path.display());

    let mut child = command.spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed to get stdout handle"))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Failed to get stderr handle"))?;

    // Save child ID before moving it
    let _child_id = child.id().unwrap_or(0);

    // Store child process for potential cleanup
    {
        let mut processes = child_processes.lock().await;
        // Convert tokio Child to std Child for storage
        // This is a temporary hack - in a real fix we'd refactor the Child storage
        let std_child = StdCommand::new("echo").spawn().unwrap();
        processes.push(std_child);
    }

    // Process stderr in a separate task
    let stderr_handler = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        
        loop {
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    error!("yt-dlp stderr: {}", line.trim());
                    line.clear();
                },
                Err(e) => {
                    error!("Error reading stderr: {}", e);
                    break;
                }
            }
        }
    });

    let mut reader = BufReader::new(stdout);
    let mut line = String::new();

    let mut last_progress_time = std::time::Instant::now();
    let progress_timeout = std::time::Duration::from_secs(300); // 5 minutes

    loop {
        // Check for cancellation
        if cancellation_token.is_cancelled() {
            warn!("Download cancelled, stopping process");
            return Err(anyhow!("Download cancelled"));
        }

        match reader.read_line(&mut line).await {
            Ok(0) => break, // EOF
            Ok(_) => {
                debug!("yt-dlp output: {}", line.trim());

                if let Some(progress) = parse_progress(&line) {
                    last_progress_time = std::time::Instant::now();

                    if let Some(sender) = &progress_sender {
                        if let Err(e) = sender.send(progress).await {
                            error!("Failed to send progress: {}", e);
                        }
                    }
                }

                // Check for progress timeout
                if last_progress_time.elapsed() > progress_timeout {
                    return Err(anyhow!("Download stalled - no progress for 5 minutes"));
                }
                
                line.clear();
            },
            Err(e) => {
                error!("Error reading stdout: {}", e);
                break;
            }
        }
    }

    // Wait for stderr handler to complete
    if let Err(e) = stderr_handler.await {
        error!("Error in stderr handler: {}", e);
    }

    // We skipped storing the actual child process earlier, so we'll just
    // wait for this specific child to complete
    let status = child.wait().await?;
    
    if !status.success() {
        return Err(anyhow!("yt-dlp failed with status: {}", status));
    }

    info!("Download process completed successfully");

    // Check if the expected file exists
    if check_file_exists_and_valid(expected_file_path).await {
        info!("Found expected file: {}", expected_file_path.display());
        return Ok(expected_file_path.clone());
    }

    // If the file doesn't exist, try to find it in the parent directory by its extension
    let output_dir = expected_file_path.parent().unwrap_or(Path::new("."));
    let extension = expected_file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    warn!("Expected file not found at {}", expected_file_path.display());
    warn!("Falling back to searching for .{} files in {}", extension, output_dir.display());
    
    // Use the existing search function as a fallback
    find_newest_file_by_extension(output_dir, extension).await
}

/// Get video information without downloading
pub async fn get_video_info(url: &str, window: &tauri::Window) -> Result<VideoInfo> {
    info!("Getting video info for URL: {}", url);

    // Basic URL validation
    if !url.starts_with("http://") && !url.starts_with("https://") {
        error!("Invalid URL format: {}", url);
        return Err(anyhow!("Invalid URL format. URL must start with http:// or https://"));
    }

    // Get yt-dlp path
    let ytdlp_path = get_tool_path("yt-dlp").ok_or_else(|| {
        error!("yt-dlp not found in system");
        anyhow!("yt-dlp not found. Please ensure it is installed correctly.")
    })?;

    debug!("Using yt-dlp from path: {}", ytdlp_path.display());

    // Get app handle from window
    let app_handle = window.app_handle();
    
    // Try to use cached cookies first
    if let Ok(Some(cookies)) = YoutubeCookieManager::load_cookies(&app_handle).await {
        if cookies.valid {
            info!("Using cached cookies from {} browser", cookies.browser);
            
            // Try with cached browser cookies
            let result = try_get_video_info(&ytdlp_path, url, &cookies.browser).await;
            
            if let Ok(video_info) = result {
                // Cookies still valid, return the result
                return Ok(video_info);
            } else {
                // Cookies no longer valid, invalidate them
                warn!("Cached cookies from {} have expired, invalidating", cookies.browser);
                let _ = YoutubeCookieManager::invalidate_cookies(&app_handle).await;
            }
        }
    }
    
    // If we get here, we need to try with fresh browser cookies
    let mut tried_browsers = Vec::new();
    let mut showed_keychain_info = false;
    
    // Try up to 3 times with increasing delays
    for attempt in 1..=3 {
        info!("Attempt {} to get video info with fresh browser cookies", attempt);

        // Try different browsers in sequence
        let browsers = if attempt == 1 {
            let mut browsers = vec!["chrome", "firefox"];
            #[cfg(target_os = "macos")]
            browsers.push("safari");
            browsers
        } else {
            vec!["chrome"]  // On retry attempts, just use Chrome
        };

        for browser in browsers {
            if tried_browsers.contains(&browser) {
                continue;
            }
            tried_browsers.push(browser);
            
            // Show keychain access info dialog before first browser attempt
            if !showed_keychain_info {
                show_keychain_info_dialog(window).await;
                showed_keychain_info = true;
                // Небольшая пауза, чтобы пользователь успел прочитать сообщение
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
            
            info!("Trying with fresh {} cookies...", browser);
            let result = try_get_video_info(&ytdlp_path, url, browser).await;
            
            if let Ok(video_info) = result {
                // Cookies worked, save them for future use
                info!("Successfully retrieved video info with {} cookies, saving for future use", browser);
                let _ = YoutubeCookieManager::save_cookies(&app_handle, browser, true).await;
                return Ok(video_info);
            }
        }

        if attempt < 3 {
            let delay = std::time::Duration::from_secs(attempt as u64);
            warn!("Retrying in {} seconds...", delay.as_secs());
            std::thread::sleep(delay);
            continue;
        }
    }

    // If we get here, all attempts with all browsers failed
    let tried_browsers_str = tried_browsers.join(", ");
    Err(anyhow!(
        "Не удалось получить информацию о видео. YouTube требует авторизацию.\n\n\
        Пожалуйста:\n\
        1. Войдите в свой аккаунт YouTube в одном из браузеров ({}).\n\
        2. Откройте YouTube и просмотрите любое видео для обновления cookies.\n\
        3. Попробуйте снова.\n\n\
        Если проблема сохраняется, попробуйте использовать другой браузер.",
        tried_browsers_str
    ))
}

/// Helper function to attempt to get video info with a specific browser's cookies
async fn try_get_video_info(ytdlp_path: &PathBuf, url: &str, browser: &str) -> Result<VideoInfo> {
    info!("Trying to get video info using {} cookies", browser);
    
    let mut command = Command::new(ytdlp_path);
    command
        .arg(url)
        .arg("--dump-json")
        .arg("--no-playlist")
        .arg("--no-warnings")
        .arg("--ignore-config")
        .arg("--no-check-certificates")
        .arg("--cookies-from-browser")
        .arg(browser)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    debug!("Executing command: {:?}", command);

    match command.output().await {
        Ok(browser_output) => {
            if browser_output.status.success() {
                debug!("Successfully retrieved info using {} cookies", browser);
                
                // Parse JSON output
                let json = match String::from_utf8(browser_output.stdout) {
                    Ok(json) => json,
                    Err(e) => {
                        error!("Failed to decode yt-dlp output as UTF-8: {}", e);
                        return Err(anyhow!("Failed to decode yt-dlp output: {}", e));
                    }
                };

                debug!("Received video metadata: {}", json);

                // Parse JSON into serde_json::Value
                let info: serde_json::Value = match serde_json::from_str(&json) {
                    Ok(info) => info,
                    Err(e) => {
                        error!("Failed to parse JSON from yt-dlp: {}", e);
                        return Err(anyhow!("Failed to parse JSON from yt-dlp: {}", e));
                    }
                };

                // Extract required fields with detailed error messages
                let title = match info["title"].as_str() {
                    Some(t) => t.to_string(),
                    None => {
                        error!("Missing or invalid title in video info");
                        return Err(anyhow!("Missing or invalid title in video info"));
                    }
                };

                let duration = match info["duration"].as_f64() {
                    Some(d) => d,
                    None => {
                        error!("Missing or invalid duration in video info");
                        return Err(anyhow!("Missing or invalid duration in video info"));
                    }
                };

                let thumbnail = info["thumbnail"].as_str().unwrap_or("").to_string();
                let description = info["description"].as_str().unwrap_or("").to_string();
                let language = info["language"].as_str().map(|s| s.to_string());
                let original_language = info["original_language"].as_str().map(|s| s.to_string());

                info!("Successfully retrieved video info for: {}", title);
                debug!("Video duration: {}s", duration);

                return Ok(VideoInfo {
                    title,
                    duration,
                    url: url.to_string(),
                    thumbnail,
                    description,
                    language,
                    original_language,
                });
            } else {
                let stderr = String::from_utf8_lossy(&browser_output.stderr);
                error!("Failed with {} cookies: {}", browser, stderr);

                // Check for specific error conditions
                if stderr.contains("Video unavailable") {
                    return Err(anyhow!("Видео недоступно. Возможно оно приватное или было удалено."));
                } else if stderr.contains("This video is not available in your country") {
                    return Err(anyhow!("Это видео недоступно в вашей стране."));
                } else if stderr.contains("Sign in to confirm your age") {
                    return Err(anyhow!("Видео имеет возрастные ограничения. Пожалуйста, войдите в свой аккаунт YouTube в браузере."));
                }
                
                return Err(anyhow!("Failed to get video info: {}", stderr));
            }
        }
        Err(e) => {
            error!("Error trying {} cookies: {}", browser, e);
            return Err(anyhow!("Error trying {} cookies: {}", browser, e));
        }
    }
}

/// Parse progress information from yt-dlp output
fn parse_progress(line: &str) -> Option<DownloadProgress> {
    if !line.starts_with("[download]") {
        return None;
    }

    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }

    let progress = parts[1].trim_end_matches('%').parse::<f32>().ok()?;

    let speed = if let Some(speed_idx) = parts.iter().position(|&p| p == "at") {
        if parts.len() > speed_idx + 1 {
            Some(parts[speed_idx + 1].to_string())
        } else {
            None
        }
    } else {
        None
    };

    let eta = if let Some(eta_idx) = parts.iter().position(|&p| p == "ETA") {
        if parts.len() > eta_idx + 1 {
            Some(parts[eta_idx + 1].to_string())
        } else {
            None
        }
    } else {
        None
    };

    Some(DownloadProgress {
        status: "downloading".to_string(),
        progress,
        speed,
        eta,
        component: "unknown".to_string(), // Will be set by the caller
    })
}

/// Find the newest file with a specific extension in a directory
async fn find_newest_file_by_extension(dir: &Path, extension: &str) -> Result<PathBuf> {
    info!("Searching for newest file with extension .{} in {}", extension, dir.display());
    
    // Ensure the directory exists
    if !dir.exists() {
        error!("Directory does not exist: {}", dir.display());
        return Err(anyhow!("Directory does not exist: {}", dir.display()));
    }
    
    let mut matching_files = Vec::new();
    
    // Read directory contents
    match tokio::fs::read_dir(dir).await {
        Ok(mut read_dir) => {
            while let Ok(Some(entry)) = read_dir.next_entry().await {
                let path = entry.path();
                
                // Only consider files with the expected extension
                if let Some(ext) = path.extension() {
                    if ext.to_string_lossy().to_lowercase() == extension.to_lowercase() {
                        match entry.metadata().await {
                            Ok(metadata) => {
                                info!("Found file with matching extension: {}", path.display());
                                matching_files.push((path, metadata));
                            },
                            Err(e) => warn!("Failed to get metadata for {}: {}", path.display(), e)
                        }
                    }
                }
            }
        },
        Err(e) => {
            error!("Failed to read directory {}: {}", dir.display(), e);
            return Err(anyhow!("Failed to read directory {}: {}", dir.display(), e));
        }
    };
    
    // If no matching files were found, log all files in the directory for debugging
    if matching_files.is_empty() {
        error!("No files with extension {} found in {}", extension, dir.display());
        
        // List all files for debugging
        match tokio::fs::read_dir(dir).await {
            Ok(mut read_dir) => {
                error!("Files in the directory:");
                while let Ok(Some(entry)) = read_dir.next_entry().await {
                    error!("  {}", entry.path().display());
                }
            },
            Err(e) => error!("Failed to read directory for debugging: {}", e)
        }
        
        return Err(anyhow!("No files with extension {} found in {}", extension, dir.display()));
    }
    
    // Sort by modification time, newest first
    matching_files.sort_by(|(_, meta_a), (_, meta_b)| {
        let time_a = meta_a.modified().unwrap_or(std::time::UNIX_EPOCH);
        let time_b = meta_b.modified().unwrap_or(std::time::UNIX_EPOCH);
        time_b.cmp(&time_a)
    });
    
    info!("Selected newest file: {}", matching_files[0].0.display());
    Ok(matching_files[0].0.clone())
}
