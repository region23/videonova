use log::{error, info, warn};
use reqwest;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::thread;
use tauri::Emitter;
use tokio::sync::mpsc;
use serde_json::json;
use futures::FutureExt;
use tts_sync::TtsSync;

use crate::utils::common::{sanitize_filename, check_file_exists_and_valid};
use crate::utils::transcribe;
use crate::utils::translate;
use crate::utils::youtube::{self, DownloadProgress, VideoInfo};

#[derive(Clone, Serialize)]
pub struct DownloadState {
    progress: DownloadProgress,
    #[serde(skip)]
    progress_sender: mpsc::Sender<DownloadProgress>,
}

#[derive(Clone, Serialize)]
pub struct DownloadResult {
    video_path: String,
    audio_path: String,
}

#[derive(Serialize)]
pub struct TranscriptionResult {
    vtt_path: String,
}

#[derive(Serialize)]
pub struct TranslationResult {
    translated_vtt_path: String,
    base_filename: String,
}

#[derive(Serialize)]
pub struct TTSResult {
    audio_path: String,
}

#[derive(Serialize)]
pub struct ProcessVideoResult {
    video_path: String,
    audio_path: String,
    transcription_path: String,
    translation_path: String,
    tts_path: String,
    final_path: String,
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
    const MAX_RETRIES: u32 = 3;
    let mut current_attempt = 0;

    loop {
        current_attempt += 1;
        info!(
            "Starting download attempt {} of {}",
            current_attempt, MAX_RETRIES
        );

        // Create progress channel for this attempt
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
        let progress_handle = tokio::spawn(async move {
            while let Some(progress) = rx.recv().await {
                // Emit progress event to frontend
                if let Err(e) = progress_window.emit("download-progress", progress.clone()) {
                    error!("Failed to emit progress: {}", e);
                }

                // Check if audio download is complete
                if progress.component == "audio"
                    && progress.progress >= 99.0
                    && !audio_completed_clone.load(Ordering::Relaxed)
                {
                    // Mark as completed to avoid duplicate events
                    audio_completed_clone.store(true, Ordering::Relaxed);

                    let event_window = progress_window.clone();
                    let url_event = url_clone.clone();
                    let output_path_event = output_path_clone.clone();

                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                        if let Ok(info) = youtube::get_video_info(&url_event).await {
                            let output_dir = PathBuf::from(&output_path_event);
                            let safe_title = sanitize_filename(&info.title);
                            let audio_path = output_dir.join(format!("{}_audio.m4a", safe_title));

                            if let Err(e) = event_window
                                .emit("audio-ready", audio_path.to_string_lossy().to_string())
                            {
                                error!("Failed to emit audio-ready event: {}", e);
                            }
                        }
                    });
                }
            }
        });

        // Start download
        let output_dir = PathBuf::from(output_path.clone());
        let result = match youtube::download_video(&url, &output_dir, Some(tx)).await {
            Ok(result) => {
                // Verify downloaded files
                let video_exists = check_file_exists_and_valid(&result.video_path).await;
                let audio_exists = check_file_exists_and_valid(&result.audio_path).await;

                if !video_exists || !audio_exists {
                    error!("Download verification failed:");
                    error!("  Video file exists and valid: {}", video_exists);
                    error!("  Audio file exists and valid: {}", audio_exists);

                    if current_attempt < MAX_RETRIES {
                        warn!("Retrying download...");
                        // Небольшая пауза перед следующей попыткой
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        continue;
                    } else {
                        return Err(format!(
                            "Failed to download after {} attempts. Files missing or empty.",
                            MAX_RETRIES
                        ));
                    }
                }

                info!(
                    "Download completed successfully on attempt {}",
                    current_attempt
                );
                info!(
                    "  Video path: {} (exists and valid)",
                    result.video_path.to_string_lossy(),
                );
                info!(
                    "  Audio path: {} (exists and valid)",
                    result.audio_path.to_string_lossy(),
                );

                // Wait for progress monitoring to complete
                let _ = progress_handle.await;

                // Create download result
                let download_result = DownloadResult {
                    video_path: result.video_path.to_string_lossy().to_string(),
                    audio_path: result.audio_path.to_string_lossy().to_string(),
                };

                // Emit download-complete event
                if let Err(e) = window.emit("download-complete", download_result.clone()) {
                    error!("Failed to emit download-complete event: {}", e);
                }

                Ok(download_result)
            }
            Err(e) => {
                error!("Download failed: {}", e);
                if current_attempt < MAX_RETRIES {
                    warn!("Retrying download...");
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    continue;
                } else {
                    Err(format!("Failed to download after {} attempts: {}", MAX_RETRIES, e))
                }
            }
        };

        // Return the result
        return result;
    }
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

    let result_path =
        transcribe::transcribe_audio(&audio_file, &output_dir, &api_key, language, Some(tx))
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
    info!("Beginning OpenAI API key validation");

    // Create a client with detailed debug information
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("videonova-tts-client/1.0")
        .build()
        .unwrap_or_else(|e| {
            warn!("Could not create custom client, using default: {}", e);
            reqwest::Client::new()
        });

    info!("Sending test request to OpenAI API");
    
    let request_start = std::time::Instant::now();
    let response = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await;
    let request_duration = request_start.elapsed();
    
    info!("OpenAI API request took {} milliseconds", request_duration.as_millis());
    
    match response {
        Ok(resp) => {
            let status = resp.status();
            info!("OpenAI API response status: {}", status);
            
            if !status.is_success() {
                // Try to get detailed error info
                // Clone resp to avoid moving the original
                match resp.text().await {
                    Ok(text) => {
                        error!("OpenAI API error response: {}", text);
                    },
                    Err(e) => {
                        error!("Could not read OpenAI API error response: {}", e);
                    }
                }
            }
            
            Ok(status.is_success())
        },
        Err(e) => {
            error!("OpenAI API request failed: {}", e);
            
            // Additional network diagnostics
            if e.is_timeout() {
                error!("Request timed out - possible network issue");
            } else if e.is_connect() {
                error!("Connection error - possible firewall or proxy issue");
            } else if e.is_request() {
                error!("Request building error - possible TLS or library issue");
            }
            
            Err(e.to_string())
        }
    }
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
    window: tauri::Window,
) -> Result<TranslationResult, String> {
    info!("Starting VTT translation to {}", target_language);
    
    // Create progress channel
    let (tx, mut rx) = mpsc::channel::<translate::TranslationProgress>(32);

    // Clone window handle for the progress monitoring task
    let progress_window = window.clone();

    // Spawn progress monitoring task
    let monitoring_task = tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            // Emit progress event to frontend
            if let Err(e) = progress_window.emit("translation-progress", progress) {
                error!("Failed to emit translation progress: {}", e);
            }
        }
        // Отправляем событие о завершении мониторинга
        if let Err(e) = progress_window.emit("translation-monitoring-complete", ()) {
            error!("Failed to emit translation monitoring complete: {}", e);
        }
    });

    // Start translation
    let vtt_file = PathBuf::from(vtt_path);
    let output_dir = PathBuf::from(output_path);

    let result_path = translate::translate_vtt(
        &vtt_file,
        &output_dir,
        &target_language_code,
        &target_language,
        &api_key,
        Some(tx),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Дожидаемся завершения задачи мониторинга
    let _ = monitoring_task.await;

    // Extract the base filename for use in generate_speech
    let filename = vtt_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    Ok(TranslationResult {
        translated_vtt_path: result_path.to_string_lossy().to_string(),
        base_filename: filename.to_string(),
    })
}

struct TauriProgressObserver {
    window: tauri::Window,
}

impl TauriProgressObserver {
    fn new(window: tauri::Window) -> Self {
        Self { window }
    }
}

/// Enhanced TTS function with detailed logging for troubleshooting
async fn enhanced_tts_with_logging(
    video_path: &str,
    audio_path: &str,
    original_vtt_path: &str,
    translated_vtt_path: &str,
    output_path: &str,
    api_key: &str,
    observer: TauriProgressObserver,
) -> Result<String, String> {
    info!("Starting enhanced TTS with detailed logging");
    
    // Log file sizes and existence for debugging
    for (path, desc) in [
        (video_path, "video"),
        (audio_path, "audio"),
        (original_vtt_path, "original subtitles"),
        (translated_vtt_path, "translated subtitles"),
    ] {
        match tokio::fs::metadata(path).await {
            Ok(metadata) => {
                info!("File check: {} ({}) - Size: {} bytes", path, desc, metadata.len());
                // Log the first few lines of the subtitle files to check format
                if desc.contains("subtitles") {
                    match tokio::fs::read_to_string(path).await {
                        Ok(content) => {
                            let preview: String = content.lines().take(10).collect::<Vec<&str>>().join("\n");
                            info!("Subtitle file preview ({}):\n{}", desc, preview);
                        },
                        Err(e) => warn!("Could not read subtitle file for preview: {}", e),
                    }
                }
            },
            Err(e) => warn!("Could not access file metadata for {}: {}", path, e),
        }
    }
    
    // Get video duration from the file
    let video_duration = match get_video_duration(video_path).await {
        Ok(duration) => {
            info!("Video duration: {:.2} seconds", duration);
            duration
        },
        Err(e) => {
            error!("Failed to get video duration: {}", e);
            return Err(format!("Failed to get video duration: {}", e));
        }
    };
    
    // Use a detailed try/catch approach to identify where issues occur
    info!("About to start TTS sync process - this is where we often get stuck");
    
    // Create a channel to communicate between threads
    let (tx, mut rx) = mpsc::channel(1);
    
    // Clone all the values we need to pass to the thread
    let translated_vtt_path_clone = translated_vtt_path.to_string();
    let api_key_clone = api_key.to_string();
    let output_path_clone = output_path.to_string();
    let window_clone = observer.window.clone();
    
    // Spawn a new thread to run the TTS synchronization
    thread::spawn(move || {
        // Create a runtime for the thread
        match tokio::runtime::Runtime::new() {
            Ok(rt) => {
                // Run the TTS synchronization in the runtime
                rt.block_on(async {
                    // Create a progress callback
                    let window = window_clone;
                    let progress_state = Arc::new(std::sync::Mutex::new(0.0f32));
                    let progress_callback = Box::new(move |progress: f32, status: &str| {
                        // Убедимся, что прогресс в диапазоне 0-100
                        let normalized_progress = progress.max(0.0).min(100.0);
                        
                        // Увеличиваем значения прогресса, чтобы гарантировать движение ползунка
                        // Множитель 1.0 для тестов, но при необходимости его можно изменить
                        let scaled_progress = normalized_progress * 1.0;
                        
                        let should_send = {
                            // Получаем доступ к предыдущему прогрессу
                            let mut previous_progress = match progress_state.lock() {
                                Ok(guard) => guard,
                                Err(_) => return, // В случае ошибки просто выходим
                            };
                            
                            // Проверяем нужно ли отправлять обновление
                            let should_update = 
                                (scaled_progress - *previous_progress).abs() >= 0.5 || 
                                scaled_progress == 0.0 || scaled_progress >= 99.9 ||
                                status.contains("завершена");
                            
                            // Обновляем значение предыдущего прогресса
                            if should_update {
                                *previous_progress = scaled_progress;
                            }
                            
                            should_update
                        };
                        
                        // Отправляем обновления только если нужно
                        if should_send {
                            // Парсим информацию о сегментах
                            let (current_segment, total_segments) = if status.contains("/") {
                                let parts: Vec<&str> = status.split("/").collect();
                                if parts.len() >= 2 {
                                    let current = parts[0].split_whitespace()
                                        .last()
                                        .and_then(|num| num.parse::<i32>().ok());
                                    
                                    let total = parts[1].split_whitespace()
                                        .next()
                                        .and_then(|num| num.parse::<i32>().ok());
                                    
                                    (current, total)
                                } else {
                                    (None, None)
                                }
                            } else {
                                (None, None)
                            };
                            
                            // Создаем объект прогресса
                            let progress_json = json!({
                                "step": "TTS Generation",
                                "step_progress": scaled_progress,
                                "total_progress": scaled_progress,
                                "details": status,
                                "current_segment": current_segment,
                                "total_segments": total_segments,
                                "timestamp": chrono::Utc::now().timestamp_millis(),
                                "status": status  // явно добавим статус для UI
                            });
                            
                            // Всегда логгируем прогресс для отладки
                            info!("TTS progress: {:.1}%, status={}", scaled_progress, status);
                            
                            // Отправляем событие
                            if let Err(e) = window.emit("tts-progress", progress_json.clone()) {
                                error!("Failed to emit TTS progress: {}", e);
                            }
                        }
                    });
                    
                    // Create a TTS sync instance with the fluent interface
                    let tts_sync = TtsSync::default()
                        .with_progress_callback(progress_callback)
                        .with_compression(true)
                        .with_equalization(true)
                        .with_volume_normalization(true)
                        .with_preserve_pauses(true);
                    
                    info!("Starting TTS synchronization with video duration: {:.2}s", video_duration);
                    let result = tts_sync.synchronize(
                        &translated_vtt_path_clone,
                        video_duration,
                        &api_key_clone,
                    ).await;
                    
                    match result {
                        Ok(output_file) => {
                            info!("TTS process completed successfully!");
                            info!("Generated TTS output file: {}", output_file);
                            
                            // Verify the generated file exists and has content
                            match tokio::fs::metadata(&output_file).await {
                                Ok(metadata) => {
                                    let file_size = metadata.len();
                                    info!("Generated file size: {} bytes", file_size);
                                    
                                    if file_size < 1000 {  // Если файл меньше 1KB, вероятно, он пуст или повреждён
                                        let error_msg = format!("Generated audio file is too small ({}B): {}", file_size, output_file);
                                        error!("{}", error_msg);
                                        let _ = tx.send(Err(error_msg)).await;
                                        return;
                                    }
                                },
                                Err(e) => {
                                    let error_msg = format!("Failed to check generated file: {}", e);
                                    error!("{}", error_msg);
                                    let _ = tx.send(Err(error_msg)).await;
                                    return;
                                }
                            }
                            
                            // Create parent directories for output path
                            let output_dir = std::path::Path::new(&output_path_clone).parent();
                            if let Some(dir) = output_dir {
                                if !dir.exists() {
                                    if let Err(e) = tokio::fs::create_dir_all(dir).await {
                                        let error_msg = format!("Failed to create output directory: {}", e);
                                        error!("{}", error_msg);
                                        let _ = tx.send(Err(error_msg)).await;
                                        return;
                                    }
                                }
                            }
                            
                            // Copy the generated audio file to the output path
                            info!("Copying from {} to {}", &output_file, &output_path_clone);
                            match tokio::fs::copy(&output_file, &output_path_clone).await {
                                Ok(bytes_copied) => {
                                    info!("Copied TTS output to: {} ({} bytes)", output_path_clone, bytes_copied);
                                    
                                    // Verify copied file
                                    match tokio::fs::metadata(&output_path_clone).await {
                                        Ok(metadata) => {
                                            if metadata.len() > 0 {
                                                info!("Verified output file: {} ({} bytes)", 
                                                      output_path_clone, metadata.len());
                                                let _ = tx.send(Ok(output_path_clone.clone())).await;
                                            } else {
                                                let error_msg = format!("Output file is empty after copy: {}", output_path_clone);
                                                error!("{}", error_msg);
                                                let _ = tx.send(Err(error_msg)).await;
                                            }
                                        },
                                        Err(e) => {
                                            let error_msg = format!("Failed to verify output file after copy: {}", e);
                                            error!("{}", error_msg);
                                            let _ = tx.send(Err(error_msg)).await;
                                        }
                                    }
                                },
                                Err(e) => {
                                    error!("Failed to copy TTS output: {}", e);
                                    let _ = tx.send(Err(format!("Failed to copy TTS output: {}", e))).await;
                                }
                            }
                        },
                        Err(e) => {
                            error!("TTS process returned an error: {:?}", e);
                            let _ = tx.send(Err(format!("TTS error: {:?}", e))).await;
                        }
                    }
                });
            },
            Err(e) => {
                let error_msg = format!("Failed to create runtime in TTS thread: {}", e);
                error!("{}", error_msg);
                
                // Don't call await here, just log the error
                // We'll handle the error with the timeout mechanism
            }
        }
    });
    
    // Wait for the result from the spawned thread
    // Add a timeout to prevent hanging indefinitely
    match tokio::time::timeout(
        std::time::Duration::from_secs(600), // 10 minute timeout
        rx.recv()
    ).await {
        Ok(Some(result)) => result,
        Ok(None) => {
            error!("TTS process channel closed unexpectedly");
            Err("TTS process failed - channel closed unexpectedly".to_string())
        },
        Err(_) => {
            error!("TTS process timed out after 10 minutes");
            Err("TTS process timed out - likely stuck in API request or processing".to_string())
        }
    }
}

// Helper function to get video duration
async fn get_video_duration(video_path: &str) -> Result<f64, String> {
    use tokio::process::Command;
    
    // Using ffprobe to get video duration
    let output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            video_path
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to execute ffprobe: {}", e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe error: {}", stderr));
    }
    
    let duration_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    duration_str.parse::<f64>().map_err(|e| format!("Failed to parse duration: {}", e))
}

// Helper function to copy the generated file to the specified output path
async fn copy_to_output_path(source: &str, destination: &str) -> Result<(), String> {
    let dest_path = std::path::Path::new(destination);
    
    // Ensure parent directories exist
    if let Some(parent) = dest_path.parent() {
        if !parent.exists() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create parent directories: {}", e))?;
        }
    }
    
    // Check if destination is a directory
    if dest_path.exists() && dest_path.is_dir() {
        // If destination is a directory, append the source filename
        let source_filename = std::path::Path::new(source)
            .file_name()
            .ok_or_else(|| "Source has no filename".to_string())?;
        
        let new_dest = dest_path.join(source_filename);
        info!("Destination is a directory, copying to: {}", new_dest.display());
        
        tokio::fs::copy(source, new_dest)
            .await
            .map(|_| ())
            .map_err(|e| format!("Failed to copy file: {}", e))
    } else {
        // Normal file copy
        tokio::fs::copy(source, dest_path)
            .await
            .map(|_| ())
            .map_err(|e| format!("Failed to copy file: {}", e))
    }
}

#[tauri::command]
pub async fn generate_speech(
    video_path: String,
    audio_path: String,
    original_vtt_path: String,
    translated_vtt_path: String,
    output_path: String,
    api_key: String,
    window: tauri::Window,
) -> Result<TTSResult, String> {
    info!("Starting TTS generation with synchronization");
    
    // Validate the API key first before proceeding
    info!("Validating OpenAI API key before TTS generation");
    if api_key.trim().is_empty() {
        error!("OpenAI API key is empty");
        return Err("OpenAI API key is required for TTS generation".to_string());
    }
    
    // Additional validation by making a test request to the OpenAI API
    match validate_openai_key(api_key.clone()).await {
        Ok(true) => info!("OpenAI API key validated successfully"),
        Ok(false) => {
            error!("Invalid OpenAI API key: Authentication failed");
            return Err("OpenAI API key validation failed. Please check your API key and ensure it has access to TTS services.".to_string());
        },
        Err(e) => {
            error!("OpenAI API key validation error: {}", e);
            return Err(format!("Failed to validate OpenAI API key: {}. Please check your internet connection and try again.", e));
        }
    }
    
    // Validate input files
    for (path, desc) in [
        (&video_path, "video"),
        (&audio_path, "audio"),
        (&original_vtt_path, "original subtitles"),
        (&translated_vtt_path, "translated subtitles"),
    ] {
        if !check_file_exists(path).await {
            error!("File not found: {} ({})", path, desc);
            return Err(format!("Required {} file not found: {}", desc, path));
        }
    }
    
    // Ensure the output path is valid
    let output_path_obj = std::path::Path::new(&output_path);
    let final_output_path = if output_path_obj.is_dir() {
        // If output_path is a directory, create a filename based on the input
        let base_name = std::path::Path::new(&translated_vtt_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "tts_output".to_string());
        
        output_path_obj.join(format!("{}_tts.mp4", base_name)).to_string_lossy().to_string()
    } else {
        // Make sure parent directories exist
        if let Some(parent) = output_path_obj.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| format!("Failed to create output directory: {}", e))?;
            }
        }
        output_path.clone()
    };
    
    info!("Final TTS output will be saved to: {}", final_output_path);
    
    // Create progress observer
    let observer = TauriProgressObserver::new(window.clone());
    
    // Use our enhanced TTS function with detailed logging
    match enhanced_tts_with_logging(
        &video_path,
        &audio_path,
        &original_vtt_path,
        &translated_vtt_path,
        &final_output_path,
        &api_key,
        observer,
    ).await {
        Ok(_) => {
            info!("TTS generation completed successfully");
            Ok(TTSResult {
                audio_path: final_output_path,
            })
        },
        Err(e) => {
            error!("TTS generation failed: {}", e);
            Err(e)
        }
    }
}

/// Helper function to check if a file exists and is valid
async fn check_file_exists(path: impl AsRef<std::path::Path>) -> bool {
    tokio::fs::metadata(path).await.is_ok()
}

/// Check if a file exists and is accessible
#[tauri::command]
pub async fn check_file_exists_command(path: String) -> Result<bool, String> {
    Ok(check_file_exists(path).await)
}

/// Process video through all steps: download, transcribe, translate, and TTS with synchronization
#[tauri::command]
pub async fn process_video(
    url: String,
    output_path: String,
    target_language: String,
    target_language_name: String,
    api_key: String,
    window: tauri::Window,
) -> Result<ProcessVideoResult, String> {
    info!("=== Starting Video Processing Pipeline ===");
    info!("Parameters:");
    info!("  URL: {}", url);
    info!("  Output Path: {}", output_path);
    info!(
        "  Target Language: {} ({})",
        target_language_name, target_language
    );

    // Step 1: Download video
    info!("Step 1: Downloading video");
    let download_result =
        match download_video(url.clone(), output_path.clone(), window.clone()).await {
            Ok(result) => {
                info!("Download completed successfully");
                info!("  Video path: {}", result.video_path);
                info!("  Audio path: {}", result.audio_path);
                result
            }
            Err(e) => {
                error!("Download failed: {}", e);
                return Err(format!("Download failed: {}", e));
            }
        };

    // Step 2: Transcribe audio
    info!("Step 2: Transcribing audio");
    let transcription_result = match transcribe_audio(
        download_result.audio_path.clone(),
        output_path.clone(),
        api_key.clone(),
        None, // language - auto detect
        window.clone(),
    )
    .await
    {
        Ok(result) => {
            info!("Transcription completed successfully");
            info!("  VTT path: {}", result.vtt_path);
            result
        }
        Err(e) => {
            error!("Transcription failed: {}", e);
            return Err(format!("Transcription failed: {}", e));
        }
    };

    // Step 3: Translate VTT
    info!("Step 3: Translating subtitles");
    let translation_result = match translate_vtt(
        transcription_result.vtt_path.clone(),
        output_path.clone(),
        "auto".to_string(),           // source language - auto detect
        target_language_name.clone(), // target language name
        target_language.clone(),      // target language code
        api_key.clone(),
        window.clone(),
    )
    .await
    {
        Ok(result) => {
            info!("Translation completed successfully");
            info!("  Translated VTT path: {}", result.translated_vtt_path);
            result
        }
        Err(e) => {
            error!("Translation failed: {}", e);
            return Err(format!("Translation failed: {}", e));
        }
    };

    // Небольшая пауза после завершения перевода и проверка файлов
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Проверяем наличие всех необходимых файлов перед запуском TTS
    for path_str in [
        &download_result.video_path,
        &download_result.audio_path,
        &transcription_result.vtt_path,
        &translation_result.translated_vtt_path,
    ] {
        let path = std::path::Path::new(path_str);
        if !check_file_exists_and_valid(path).await {
            let error_msg = format!("Required file not found or empty: {}", path_str);
            error!("{}", error_msg);
            return Err(error_msg);
        }
    }

    // Step 4: Generate TTS and synchronize with video
    info!("Step 4: Generating speech and synchronizing with video");
    
    // Создаем отдельную директорию для финального результата
    let final_dir = PathBuf::from(&output_path).join("final");
    tokio::fs::create_dir_all(&final_dir)
        .await
        .map_err(|e| format!("Failed to create final directory: {}", e))?;
    
    // Use a filename, not just a directory
    let original_filename = std::path::Path::new(&download_result.video_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "video".to_string());
    
    let final_output = final_dir.join(format!("{}_final.mp4", original_filename));
    info!("Final output will be saved to: {}", final_output.display());

    let tts_result = generate_speech(
        download_result.video_path.clone(),
        download_result.audio_path.clone(),
        transcription_result.vtt_path.clone(),
        translation_result.translated_vtt_path.clone(),
        final_output.to_string_lossy().to_string(),
        api_key.clone(),
        window.clone(),
    )
    .await
    .map_err(|e| {
        error!("TTS generation and synchronization failed: {}", e);
        format!("TTS generation and synchronization failed: {}", e)
    })?;

    // Verify the output file was created
    let final_file_path = std::path::Path::new(&tts_result.audio_path);
    if !final_file_path.exists() {
        let error_msg = format!("Final output file was not created: {}", tts_result.audio_path);
        error!("{}", error_msg);
        return Err(error_msg);
    }
    
    let final_file_size = tokio::fs::metadata(final_file_path).await
        .map(|m| m.len())
        .unwrap_or(0);
    
    if final_file_size == 0 {
        let error_msg = format!("Final output file is empty: {}", tts_result.audio_path);
        error!("{}", error_msg);
        return Err(error_msg);
    }
    
    info!("Final file size: {} bytes", final_file_size);
    info!("=== Video Processing Pipeline Completed Successfully ===");
    info!("Final video saved to: {}", tts_result.audio_path);

    Ok(ProcessVideoResult {
        video_path: download_result.video_path,
        audio_path: download_result.audio_path,
        transcription_path: transcription_result.vtt_path,
        translation_path: translation_result.translated_vtt_path,
        tts_path: tts_result.audio_path.clone(),
        final_path: tts_result.audio_path,
    })
}
