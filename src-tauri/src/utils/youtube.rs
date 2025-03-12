use anyhow::{Result, anyhow};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio::task;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tauri::{Manager, Emitter};

use super::tools::get_tool_path;
use crate::utils::common::{sanitize_filename, check_file_exists_and_valid};

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
    
    // Get video info first to get the title
    info!("Fetching video information...");
    let video_info = get_video_info(url, window).await?;
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

    // Create output directory if it doesn't exist
    std::fs::create_dir_all(output_dir)?;
    debug!("Ensured output directory exists");

    // Store child processes for cleanup
    let child_processes = Arc::new(Mutex::new(Vec::new()));
    let child_processes_clone = child_processes.clone();

    // Prepare output templates with yt-dlp's --restrict-filenames for consistency
    // We'll use constant extensions for predictability (m4a for audio, mp4 for video)
    let audio_filename = format!("{}_audio.m4a", safe_title);
    let video_filename = format!("{}_video.mp4", safe_title);
    
    let audio_template = output_dir.join(format!("{}_audio.%(ext)s", safe_title));
    let video_template = output_dir.join(format!("{}_video.%(ext)s", safe_title));
    
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
    let audio_path = audio_result?;
    let video_path = video_result?;

    // Verify downloaded files
    let video_exists = check_file_exists_and_valid(&video_path).await;
    let audio_exists = check_file_exists_and_valid(&audio_path).await;

    if !video_exists || !audio_exists {
        error!("Download verification failed:");
        error!("  Video file exists and valid: {}", video_exists);
        error!("  Audio file exists and valid: {}", audio_exists);
        return Err(anyhow!("Downloaded files are missing or empty"));
    }

    info!("Download completed successfully");
    debug!("Audio file: {}", audio_path.display());
    debug!("Video file: {}", video_path.display());

    Ok(DownloadResult {
        video_path,
        audio_path,
    })
}

/// Download audio only
async fn download_audio(
    ytdlp_path: &PathBuf,
    url: &str,
    output_template: &PathBuf,
    progress_sender: Option<mpsc::Sender<DownloadProgress>>,
    cancellation_token: CancellationToken,
    child_processes: Arc<Mutex<Vec<std::process::Child>>>,
) -> Result<PathBuf> {
    info!("Starting audio download for URL: {}", url);
    debug!("Using output template: {}", output_template.display());

    // Extract the expected filename pattern from the output template
    let filename_pattern = output_template
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.replace("%(ext)s", "m4a"))
        .unwrap_or_else(|| "_audio.m4a".to_string());
    
    debug!("Expected filename pattern: {}", filename_pattern);

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
        &filename_pattern,
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
    child_processes: Arc<Mutex<Vec<std::process::Child>>>,
) -> Result<PathBuf> {
    info!("Starting video-only download for URL: {}", url);
    debug!("Using output template: {}", output_template.display());
    
    // Extract the expected filename pattern from the output template
    let filename_pattern = output_template
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.replace("%(ext)s", "mp4"))
        .unwrap_or_else(|| "_video.mp4".to_string());
    
    debug!("Expected filename pattern: {}", filename_pattern);

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
        &filename_pattern,
    )
    .await
}

/// Process download command and handle progress
async fn process_download(
    mut command: Command,
    progress_sender: Option<mpsc::Sender<DownloadProgress>>,
    cancellation_token: CancellationToken,
    child_processes: Arc<Mutex<Vec<std::process::Child>>>,
    expected_filename_pattern: &str,
) -> Result<PathBuf> {
    debug!("Starting download process with command: {:?}", command);
    debug!("Expected filename pattern: {}", expected_filename_pattern);

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
    let child_id = child.id();

    // Store child process for potential cleanup
    {
        let mut processes = child_processes.lock().await;
        processes.push(child);
    }

    // Process stderr in a separate task
    let stderr_handler = tokio::spawn(async move {
        let reader = std::io::BufReader::new(stderr);
        let lines = std::io::BufRead::lines(reader);
        for line in lines {
            if let Ok(line) = line {
                error!("yt-dlp stderr: {}", line);
            }
        }
    });

    let reader = std::io::BufReader::new(stdout);
    let lines = std::io::BufRead::lines(reader);

    let mut last_progress_time = std::time::Instant::now();
    let progress_timeout = std::time::Duration::from_secs(300); // 5 minutes

    for line in lines {
        // Check for cancellation
        if cancellation_token.is_cancelled() {
            warn!("Download cancelled, stopping process");
            return Err(anyhow!("Download cancelled"));
        }

        if let Ok(line) = line {
            debug!("yt-dlp output: {}", line);

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
        }
    }

    // Wait for stderr handler to complete
    if let Err(e) = stderr_handler.await {
        error!("Error in stderr handler: {}", e);
    }

    // Remove child process from the list and get it back
    let mut child = {
        let mut processes = child_processes.lock().await;
        let pos = processes
            .iter()
            .position(|p| p.id() == child_id)
            .ok_or_else(|| anyhow!("Child process not found in list"))?;
        processes.remove(pos)
    };

    let status = child.wait()?;
    if !status.success() {
        return Err(anyhow!("yt-dlp failed with status: {}", status));
    }

    info!("Download process completed successfully");

    // Find the output file by pattern instead of just taking the newest file
    let parent = PathBuf::from(
        command
            .get_current_dir()
            .unwrap_or(&std::env::current_dir()?),
    );
    debug!("Searching for output file in: {}", parent.display());
    debug!("Looking for pattern: {}", expected_filename_pattern);

    let entries = std::fs::read_dir(&parent)?;
    let mut matching_files = Vec::new();

    for entry in entries {
        let entry = entry?;
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();
        
        if filename_str.contains(expected_filename_pattern) {
            debug!("Found matching file: {}", entry.path().display());
            matching_files.push((entry.path(), entry.metadata()?));
        }
    }

    // If we found matching files, prefer the newest one
    if !matching_files.is_empty() {
        matching_files.sort_by(|(_, meta_a), (_, meta_b)| {
            let time_a = meta_a.modified().unwrap_or(std::time::UNIX_EPOCH);
            let time_b = meta_b.modified().unwrap_or(std::time::UNIX_EPOCH);
            time_b.cmp(&time_a) // Sort newest first
        });
        
        let file_path = matching_files[0].0.clone();
        debug!("Selected file: {}", file_path.display());
        return Ok(file_path);
    }

    // Fallback to original method if no matching files found
    warn!("No files matching pattern '{}' found, falling back to newest file method", expected_filename_pattern);
    
    let entries = std::fs::read_dir(&parent)?;
    let mut newest_file = None;
    let mut newest_time = std::time::UNIX_EPOCH;

    for entry in entries {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if let Ok(modified) = metadata.modified() {
            if modified > newest_time {
                newest_time = modified;
                newest_file = Some(entry.path());
                debug!("Found newer file: {}", entry.path().display());
            }
        }
    }

    newest_file.ok_or_else(|| anyhow!("Failed to find downloaded file"))
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

    let mut tried_browsers = Vec::new();
    let mut showed_keychain_info = false;
    
    // Try up to 3 times with increasing delays
    for attempt in 1..=3 {
        info!("Attempt {} to get video info", attempt);

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
            
            info!("Trying with {} cookies...", browser);
            let mut command = Command::new(&ytdlp_path);
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

            match command.output() {
                Ok(browser_output) => {
                    if browser_output.status.success() {
                        debug!("Successfully retrieved info using {} cookies", browser);
                        
                        // Parse JSON output
                        let json = match String::from_utf8(browser_output.stdout) {
                            Ok(json) => json,
                            Err(e) => {
                                error!("Failed to decode yt-dlp output as UTF-8: {}", e);
                                continue;
                            }
                        };

                        debug!("Received video metadata: {}", json);

                        // Parse JSON into serde_json::Value
                        let info: serde_json::Value = match serde_json::from_str(&json) {
                            Ok(info) => info,
                            Err(e) => {
                                error!("Failed to parse JSON from yt-dlp: {}", e);
                                continue;
                            }
                        };

                        // Extract required fields with detailed error messages
                        let title = match info["title"].as_str() {
                            Some(t) => t.to_string(),
                            None => {
                                error!("Missing or invalid title in video info");
                                continue;
                            }
                        };

                        let duration = match info["duration"].as_f64() {
                            Some(d) => d,
                            None => {
                                error!("Missing or invalid duration in video info");
                                continue;
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
                    }
                }
                Err(e) => error!("Error trying {} cookies: {}", browser, e),
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
