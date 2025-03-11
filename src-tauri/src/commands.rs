use log::{error, info, warn};
use reqwest;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use tauri::Emitter;
use tokio::sync::mpsc;
use serde_json::json;
use std::path::Path;
use crate::utils::tts::tts::{synchronizer::{SyncConfig, process_sync}, ProgressUpdate, TtsConfig, AudioProcessingConfig};
use crate::utils::common::{sanitize_filename, check_file_exists_and_valid};
use crate::utils::merge::{self, MergeProgress};
use crate::utils::transcribe;
use crate::utils::translate;
use crate::utils::youtube::{self, DownloadProgress, VideoInfo};
use crate::utils::tts::tts::soundtouch;

#[derive(Clone, Serialize)]
pub struct DownloadState {
    progress: DownloadProgress,
    #[serde(skip)]
    #[allow(dead_code)]
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
    merged_path: String,
}

#[derive(Serialize)]
pub struct MergeResult {
    merged_video_path: String,
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
    _source_language: String,
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
    
    // Create a progress update channel for our custom TTS library
    let (progress_tx, mut progress_rx) = mpsc::channel(100);
    
    // Clone all the values we need to pass to the thread
    let translated_vtt_path_clone = translated_vtt_path.to_string();
    let api_key_clone = api_key.to_string();
    let output_path_clone = output_path.to_string();
    let audio_path_clone = audio_path.to_string();
    let window_clone = observer.window.clone();
    
    // Spawn a new thread to run the TTS synchronization
    thread::spawn(move || {
        // Create a runtime for the thread
        match tokio::runtime::Runtime::new() {
            Ok(rt) => {
                // Run the TTS synchronization in the runtime
                rt.block_on(async {
                    // Create a task to handle progress updates
                    let progress_window = window_clone.clone();
                    let progress_state = Arc::new(std::sync::Mutex::new(0.0f32));
                    
                    // Spawn a task to handle progress updates from the TTS library
                    let progress_task = tokio::spawn(async move {
                        while let Some(update) = progress_rx.recv().await {
                            let (progress, status, current, total) = match &update {
                                ProgressUpdate::Started => (0.0, "Начало обработки TTS".to_string(), None, None),
                                ProgressUpdate::ParsingVTT => (5.0, "Парсинг VTT файла".to_string(), None, None),
                                ProgressUpdate::ParsedVTT { total } => (10.0, format!("Найдено {} реплик", total), None, Some(*total as i32)),
                                ProgressUpdate::TTSGeneration { current, total } => {
                                    let progress = 10.0 + 50.0 * (*current as f32 / *total as f32);
                                    (progress, format!("Генерация TTS {}/{}", current, total), Some(*current as i32), Some(*total as i32))
                                },
                                ProgressUpdate::ProcessingFragment { index, total, step } => {
                                    let progress = 60.0 + 30.0 * (*index as f32 / *total as f32);
                                    (progress, format!("Обработка фрагмента {}/{}: {}", index, total, step), Some(*index as i32), Some(*total as i32))
                                },
                                ProgressUpdate::MergingFragments => (90.0, "Склейка аудиофрагментов".to_string(), None, None),
                                ProgressUpdate::Normalizing { using_original } => {
                                    let src = if *using_original { "исходный файл" } else { "целевой уровень" };
                                    (95.0, format!("Нормализация громкости (используя {})", src), None, None)
                                },
                                ProgressUpdate::Encoding => (98.0, "Кодирование в WAV".to_string(), None, None),
                                ProgressUpdate::Finished => (100.0, "Обработка TTS завершена".to_string(), None, None),
                            };
                            
                            // Убедимся, что прогресс в диапазоне 0-100
                            let normalized_progress = progress.max(0.0).min(100.0);
                            
                            let should_send = {
                                // Получаем доступ к предыдущему прогрессу
                                let mut previous_progress = match progress_state.lock() {
                                    Ok(guard) => guard,
                                    Err(_) => return, // В случае ошибки просто выходим
                                };
                                
                                // Проверяем нужно ли отправлять обновление
                                let should_update = 
                                    (normalized_progress - *previous_progress).abs() >= 0.5 || 
                                    normalized_progress == 0.0 || normalized_progress >= 99.9 ||
                                    status.contains("завершена");
                                
                                // Обновляем значение предыдущего прогресса
                                if should_update {
                                    *previous_progress = normalized_progress;
                                }
                                
                                should_update
                            };
                            
                            // Отправляем обновления только если нужно
                            if should_send {
                                // Создаем объект прогресса
                                let progress_json = json!({
                                    "step": "TTS Generation",
                                    "step_progress": normalized_progress,
                                    "total_progress": normalized_progress,
                                    "details": status,
                                    "current_segment": current,
                                    "total_segments": total,
                                    "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64,
                                    "status": status  // явно добавим статус для UI
                                });
                                
                                // Всегда логгируем прогресс для отладки
                                info!("TTS progress: {:.1}%, status={}", normalized_progress, status);
                                
                                // Отправляем событие
                                if let Err(e) = progress_window.emit("tts-progress", progress_json.clone()) {
                                    error!("Failed to emit TTS progress: {}", e);
                                }
                            }
                        }
                    });
                    
                    // Set up the configuration for our TTS library
                    let vtt_path = Path::new(&translated_vtt_path_clone);
                    let output_wav_path = Path::new(&output_path_clone);
                    let original_audio = Some(Path::new(&audio_path_clone));
                    
                    // Create TTS configuration with sensible defaults
                    let tts_config = TtsConfig {
                        model: "tts-1-hd".to_string(),
                        voice: "alloy".to_string(),
                        speed: 1.0,
                    };
                    
                    // Create audio processing configuration with sensible defaults
                    let audio_config = AudioProcessingConfig {
                        window_size: 4096,
                        hop_size: 1024,
                        target_peak_level: 0.8,
                    };
                    
                    // Create the sync configuration
                    let sync_config = SyncConfig {
                        api_key: &api_key_clone,
                        vtt_path,
                        output_wav: output_wav_path,
                        original_audio_path: original_audio,
                        progress_sender: Some(progress_tx),
                        tts_config,
                        audio_config,
                    };
                    
                    // Run the TTS synchronization
                    info!("Starting TTS synchronization with video duration: {:.2}s", video_duration);
                    match process_sync(sync_config).await {
                        Ok(()) => {
                            info!("TTS process completed successfully!");
                            info!("Generated TTS output file: {}", output_path_clone);
                            
                            // Verify the generated file exists and has content
                            match tokio::fs::metadata(&output_path_clone).await {
                                Ok(metadata) => {
                                    let file_size = metadata.len();
                                    info!("Generated file size: {} bytes", file_size);
                                    
                                    if file_size < 1000 {  // Если файл меньше 1KB, вероятно, он пуст или повреждён
                                        let error_msg = format!("Generated audio file is too small ({}B): {}", file_size, output_path_clone);
                                        error!("{}", error_msg);
                                        let _ = tx.send(Err(error_msg)).await;
                                        return;
                                    }
                                    
                                    let _ = tx.send(Ok(output_path_clone.clone())).await;
                                },
                                Err(e) => {
                                    let error_msg = format!("Failed to check generated file: {}", e);
                                    error!("{}", error_msg);
                                    let _ = tx.send(Err(error_msg)).await;
                                }
                            }
                        },
                        Err(e) => {
                            error!("TTS process returned an error: {:?}", e);
                            let _ = tx.send(Err(format!("TTS error: {:?}", e))).await;
                        }
                    }
                    
                    // Cancel the progress task since we're done
                    progress_task.abort();
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

/// Helper function to copy a file to the output path
/// This is a utility function that may be used in the future
#[allow(dead_code)]
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
    
    // Проверяем наличие SoundTouch библиотеки перед тем, как начать TTS процесс
    info!("Checking SoundTouch installation before starting TTS process");
    if let Err(e) = soundtouch::ensure_soundtouch_installed() {
        error!("SoundTouch installation check failed: {}", e);
        return Err(format!("SoundTouch library is not available: {}. Please ensure that SoundTouch is installed on your system.", e));
    }
    info!("SoundTouch is available, proceeding with TTS generation");
    
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
    
    // Make sure parent directories exist if output_path is a full file path
    if let Some(parent) = output_path_obj.parent() {
        if !parent.exists() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create output directory: {}", e))?;
        }
    }
    
    info!("TTS output will be saved to: {}", output_path);
    
    // Create progress observer
    let observer = TauriProgressObserver::new(window.clone());
    
    // Use our enhanced TTS function with detailed logging
    match enhanced_tts_with_logging(
        &video_path,
        &audio_path,
        &original_vtt_path,
        &translated_vtt_path,
        &output_path,
        &api_key,
        observer,
    ).await {
        Ok(_) => {
            info!("TTS generation completed successfully");
            Ok(TTSResult {
                audio_path: output_path,
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
    
    // Create a dedicated TTS directory for intermediate audio files
    let tts_dir = PathBuf::from(&output_path).join("tts");
    tokio::fs::create_dir_all(&tts_dir)
        .await
        .map_err(|e| format!("Failed to create TTS directory: {}", e))?;
    
    // Use a filename with correct .wav extension in the tts subdirectory
    let original_filename = std::path::Path::new(&download_result.video_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "video".to_string());
    
    // Save to tts subdirectory with .wav extension
    let tts_output = tts_dir.join(format!("{}_tts.wav", original_filename));
    info!("TTS output will be saved to: {}", tts_output.display());

    let tts_result = generate_speech(
        download_result.video_path.clone(),
        download_result.audio_path.clone(),
        transcription_result.vtt_path.clone(),
        translation_result.translated_vtt_path.clone(),
        tts_output.to_string_lossy().to_string(),
        api_key.clone(),
        window.clone(),
    )
    .await
    .map_err(|e| {
        error!("TTS generation and synchronization failed: {}", e);
        format!("TTS generation and synchronization failed: {}", e)
    })?;

    // Verify the output file was created
    let tts_file_path = std::path::Path::new(&tts_result.audio_path);
    if !tts_file_path.exists() {
        let error_msg = format!("TTS output file was not created: {}", tts_result.audio_path);
        error!("{}", error_msg);
        return Err(error_msg);
    }

    // Check file size
    let tts_file_size = tokio::fs::metadata(tts_file_path).await
        .map(|m| m.len())
        .map_err(|e| format!("Failed to get file size: {}", e))?;

    if tts_file_size == 0 {
        let error_msg = format!("TTS output file is empty: {}", tts_result.audio_path);
        error!("{}", error_msg);
        return Err(error_msg);
    }

    info!("TTS file size: {} bytes", tts_file_size);
    
    // Step 5: Merge everything together
    info!("Step 5: Merging video, audio, and subtitles");
    
    // Create a merged directory
    let merged_dir = PathBuf::from(&output_path).join("merged");
    tokio::fs::create_dir_all(&merged_dir)
        .await
        .map_err(|e| format!("Failed to create merged directory: {}", e))?;
    
    // We need to determine source language code from transcription
    let source_language_code = "auto"; // Default, should be obtained from transcription if available
    let source_language_name = "Original"; // Default
    
    let merge_result = merge_video(
        download_result.video_path.clone(),
        tts_result.audio_path.clone(), // Use the TTS result as the translated audio
        download_result.audio_path.clone(),
        transcription_result.vtt_path.clone(),
        translation_result.translated_vtt_path.clone(),
        merged_dir.to_string_lossy().to_string(),
        source_language_code.to_string(),
        target_language.clone(),
        source_language_name.to_string(),
        target_language_name.clone(),
        window.clone(),
    )
    .await
    .map_err(|e| {
        error!("Merging failed: {}", e);
        format!("Merging failed: {}", e)
    })?;
    
    // Verify the merged file was created
    let merged_file_path = std::path::Path::new(&merge_result.merged_video_path);
    if !merged_file_path.exists() {
        let error_msg = format!("Merged output file was not created: {}", merge_result.merged_video_path);
        error!("{}", error_msg);
        return Err(error_msg);
    }
    
    let merged_file_size = tokio::fs::metadata(merged_file_path).await
        .map(|m| m.len())
        .unwrap_or(0);
    
    if merged_file_size == 0 {
        let error_msg = format!("Merged output file is empty: {}", merge_result.merged_video_path);
        error!("{}", error_msg);
        return Err(error_msg);
    }
    
    info!("Merged file size: {} bytes", merged_file_size);
    info!("=== Video Processing Pipeline Completed Successfully ===");
    info!("Final video saved to: {}", tts_result.audio_path);
    info!("Merged video saved to: {}", merge_result.merged_video_path);

    Ok(ProcessVideoResult {
        video_path: download_result.video_path,
        audio_path: download_result.audio_path,
        transcription_path: transcription_result.vtt_path,
        translation_path: translation_result.translated_vtt_path,
        tts_path: tts_result.audio_path.clone(),
        final_path: tts_result.audio_path,  // Используем tts_result вместо final_output
        merged_path: merge_result.merged_video_path,
    })
}

/// Merge video with translated audio, original audio, and subtitles
pub async fn merge_video(
    video_path: String,
    translated_audio_path: String,
    original_audio_path: String,
    original_vtt_path: String,
    translated_vtt_path: String,
    output_dir: String,
    source_language_code: String,
    target_language_code: String,
    source_language_name: String,
    target_language_name: String,
    window: tauri::Window,
) -> Result<MergeResult, String> {
    info!("Starting video merging process");
    
    let (progress_tx, mut progress_rx) = mpsc::channel::<MergeProgress>(32);
    
    // Clone window for progress updates
    let window_clone = window.clone();
    
    // Spawn a task to forward progress updates to the frontend
    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            let _ = window_clone.emit("merge_progress", json!({
                "status": progress.status,
                "progress": progress.progress,
            }));
        }
    });
    
    // Convert paths to Path objects
    let video_path = Path::new(&video_path);
    let translated_audio_path = Path::new(&translated_audio_path);
    let original_audio_path = Path::new(&original_audio_path); 
    let original_vtt_path = Path::new(&original_vtt_path);
    let translated_vtt_path = Path::new(&translated_vtt_path);
    let output_dir = Path::new(&output_dir);
    
    // Call the merge_files function
    let result = merge::merge_files(
        video_path,
        translated_audio_path,
        original_audio_path,
        original_vtt_path,
        translated_vtt_path,
        output_dir,
        &source_language_code,
        &target_language_code,
        &source_language_name,
        &target_language_name,
        Some(progress_tx),
    )
    .await
    .map_err(|e| {
        error!("Merging failed: {}", e);
        format!("Merging failed: {}", e)
    })?;
    
    info!("Merging completed successfully");
    info!("  Merged video path: {}", result.display());
    
    Ok(MergeResult {
        merged_video_path: result.to_string_lossy().to_string(),
    })
}
