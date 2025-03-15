use log::{error, info, warn};
use reqwest;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use tauri::Emitter;
use tokio::sync::mpsc;
use serde_json::json;
use std::path::Path;
use tokio_util::sync::CancellationToken;
use tauri_plugin_opener::OpenerExt;
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
    output_dir: String,
}

#[derive(Debug)]
pub enum Step {
    Download { url: String },
    Transcribe,
    Translate,
    GenerateSpeech,
    Merge,
}

/// Get information about a YouTube video
#[tauri::command]
pub async fn get_video_info(window: tauri::Window, url: String) -> Result<VideoInfo, String> {
    youtube::get_video_info(&url, &window)
        .await
        .map_err(|e| e.to_string())
}

/// Start downloading a YouTube video
#[tauri::command]
pub async fn download_video(
    window: tauri::Window,
    url: String,
    output_dir: String,
) -> Result<serde_json::Value, String> {
    let (tx, mut rx) = mpsc::channel(32);
    let output_dir = PathBuf::from(output_dir);
    let cancellation_token = CancellationToken::new();
    
    // Spawn task to handle progress updates
    let window_clone = window.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            if let Err(e) = window_clone.emit("download-progress", progress) {
                error!("Failed to emit progress: {}", e);
            }
        }
    });
    
    match youtube::download_video(&url, &output_dir, Some(tx), cancellation_token, &window).await {
        Ok(result) => Ok(result.to_frontend_response()),
        Err(e) => Err(e.to_string()),
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
                        // Add a tracked highest progress value to prevent decreases
                        let mut highest_progress = 0.0f32;
                        
                        while let Some(update) = progress_rx.recv().await {
                            let (progress, status, current, total) = match &update {
                                ProgressUpdate::Started => (0.0, "Подготовка TTS".to_string(), None, None),
                                ProgressUpdate::ParsingVTT => (5.0, "Анализ субтитров".to_string(), None, None),
                                ProgressUpdate::ParsedVTT { total } => (10.0, "Субтитры готовы".to_string(), None, Some(*total as i32)),
                                ProgressUpdate::TTSGeneration { current, total } => {
                                    // Reduce the TTS generation range to leave room for vocal removal and mixing
                                    let progress = 10.0 + 40.0 * (*current as f32 / *total as f32);
                                    (progress, format!("Генерация TTS"), Some(*current as i32), Some(*total as i32))
                                },
                                ProgressUpdate::ProcessingFragment { index, total, step } => {
                                    // Limit detailed step information
                                    let simplified_step = if step.contains("Удаление вокала") {
                                        "Удаление вокала"
                                    } else if step.contains("Длительность") {
                                        "Обработка аудио"
                                    } else {
                                        &step
                                    };
                                    
                                    // For vocal removal specifically, make it finish at 85%
                                    let progress = if step.contains("Удаление вокала") {
                                        // Remap to 50-85%
                                        50.0 + 35.0 * (*index as f32 / *total as f32)
                                    } else {
                                        // Remap all other processing to go from 60% to 90% 
                                        60.0 + 30.0 * (*index as f32 / *total as f32)
                                    };
                                    
                                    (progress, format!("Обработка аудио"), Some(*index as i32), Some(*total as i32))
                                },
                                ProgressUpdate::MergingFragments => (90.0, "Формирование результата".to_string(), None, None),
                                ProgressUpdate::Normalizing { using_original } => (95.0, "Нормализация громкости".to_string(), None, None),
                                ProgressUpdate::Encoding => (98.0, "Сохранение результата".to_string(), None, None),
                                ProgressUpdate::Finished => (100.0, "TTS готов".to_string(), None, None),
                            };
                            
                            // Убедимся, что прогресс в диапазоне 0-100
                            let mut normalized_progress = progress.max(0.0).min(100.0);
                            
                            // Never decrease progress (except for new starts)
                            if normalized_progress < highest_progress && normalized_progress > 1.0 {
                                info!("Prevented progress decrease: {} -> {}", normalized_progress, highest_progress);
                                normalized_progress = highest_progress;
                            } else if normalized_progress > highest_progress {
                                highest_progress = normalized_progress;
                            }
                            
                            let should_send = {
                                // Получаем доступ к предыдущему прогрессу
                                let mut previous_progress = match progress_state.lock() {
                                    Ok(guard) => guard,
                                    Err(_) => return, // В случае ошибки просто выходим
                                };
                                
                                // Only send updates if progress has increased and exceeds a threshold, or for important status changes
                                let should_update = 
                                    (normalized_progress > *previous_progress && normalized_progress - *previous_progress >= 0.5) || 
                                    normalized_progress == 0.0 || normalized_progress >= 99.9 ||
                                    status.contains("готов");
                                
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
                                    "status": status,  // явно добавим статус для UI
                                    "progress": normalized_progress  // Явно добавляем поле progress для совместимости с интерфейсом прогресса
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
                        voice: "ash".to_string(),
                        speed: 1.0,
                    };
                    
                    // Create audio processing configuration with sensible defaults
                    let audio_config = AudioProcessingConfig {
                        window_size: 512,
                        hop_size: 256,
                        target_peak_level: 0.8,
                        voice_to_instrumental_ratio: 0.6,
                        instrumental_boost: 1.5,
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
    source_language_code: String,
    source_language_name: String,
    api_key: String,
    window: tauri::Window,
) -> Result<ProcessVideoResult, String> {
    info!("=== Starting Video Processing Pipeline ===");
    info!("Parameters:");
    info!("  URL: {}", url);
    info!("  Output Path: {}", output_path);
    info!("  Source Language: {} ({})", source_language_name, source_language_code);
    info!(
        "  Target Language: {} ({})",
        target_language_name, target_language
    );

    // Step 1: Download video
    info!("Step 1: Downloading video");
    let download_result = match download_video(window.clone(), url.clone(), output_path.clone()).await {
        Ok(json_result) => {
            let video_path = json_result["video_path"].as_str()
                .ok_or_else(|| "Missing video_path in download result".to_string())?
                .to_string();
            let audio_path = json_result["audio_path"].as_str()
                .ok_or_else(|| "Missing audio_path in download result".to_string())?
                .to_string();
            info!("Download completed successfully");
            info!("  Video path: {}", video_path);
            info!("  Audio path: {}", audio_path);
            (video_path, audio_path)
        }
        Err(e) => {
            error!("Download failed: {}", e);
            return Err(format!("Download failed: {}", e));
        }
    };

    // Step 2: Transcribe audio
    info!("Step 2: Transcribing audio");
    let transcription_result = match transcribe_audio(
        download_result.1.clone(), // audio_path
        output_path.clone(),
        api_key.clone(),
        None, // language - auto detect
        window.clone(),
    )
    .await {
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
        source_language_code.clone(),  // Use actual source language from parameters
        target_language_name.clone(), // target language name
        target_language.clone(),      // target language code
        api_key.clone(),
        window.clone(),
    )
    .await {
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
        &download_result.0, // video_path
        &download_result.1, // audio_path
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
    let tts_dir = PathBuf::from(&output_path).join("videonova_temp").join("tts");
    tokio::fs::create_dir_all(&tts_dir)
        .await
        .map_err(|e| format!("Failed to create TTS directory: {}", e))?;
    
    // Use a filename with correct .wav extension in the tts subdirectory
    let original_filename = std::path::Path::new(&download_result.0) // video_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "video".to_string());
    
    // Save to tts subdirectory with .wav extension
    let tts_output = tts_dir.join(format!("{}_tts.wav", original_filename));
    info!("TTS output will be saved to: {}", tts_output.display());

    let tts_result = generate_speech(
        download_result.0.clone(), // video_path
        download_result.1.clone(), // audio_path
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

    // We need to determine source language code from transcription
    let merge_result = merge_video(
        download_result.0.clone(), // video_path
        tts_result.audio_path.clone(), // Use the TTS result as the translated audio
        download_result.1.clone(), // audio_path
        transcription_result.vtt_path.clone(),
        translation_result.translated_vtt_path.clone(),
        output_path.clone(), // Use the user-selected output directory directly
        source_language_code,
        target_language.clone(),
        source_language_name,
        target_language_name.clone(),
        window.clone(),
    )
    .await
    .map_err(|e| {
        error!("Merging failed: {}", e);
        format!("Merging failed: {}", e)
    })?;

    info!("=== Video Processing Pipeline Completed Successfully ===");
    info!("Final video saved to: {}", merge_result.merged_video_path);
    info!("Output directory: {}", merge_result.output_dir);
    info!("TTS audio saved to: {}", tts_result.audio_path);

    // Emit merge-complete event before returning
    window.emit("merge-complete", &merge_result)
        .map_err(|e| format!("Failed to emit merge-complete event: {}", e))?;

    // Clean up temporary files
    info!("Starting cleanup of temporary files");
    if let Err(e) = cleanup_temp_files(
        merge_result.merged_video_path.clone(),
        output_path.clone()
    ).await {
        warn!("Failed to cleanup temporary files: {}", e);
        // Don't return error here, as the main process was successful
    }

    Ok(ProcessVideoResult {
        video_path: download_result.0, // video_path
        audio_path: download_result.1, // audio_path
        transcription_path: transcription_result.vtt_path,
        translation_path: translation_result.translated_vtt_path,
        tts_path: tts_result.audio_path,
        final_path: merge_result.merged_video_path.clone(),
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
            let _ = window_clone.emit("merge-progress", json!({
                "status": progress.status,
                "progress": progress.progress,
                // Add additional fields to ensure compatibility with UI
                "step": "Video Merging",
                "step_progress": progress.progress,
                "total_progress": progress.progress
            }));
        }
    });

    // Get original video filename without extension
    let video_filename = Path::new(&video_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");

    // Create final output path with language code suffix in user's selected directory
    let final_output_path = PathBuf::from(&output_dir)
        .join(format!("{}_{}.mp4", video_filename, target_language_code));

    // Create output directory if it doesn't exist
    tokio::fs::create_dir_all(&output_dir)
        .await
        .map_err(|e| format!("Failed to create output directory: {}", e))?;
    
    info!("Final output will be: {}", final_output_path.display());
    
    // Convert paths to Path objects
    let video_path = Path::new(&video_path);
    let translated_audio_path = Path::new(&translated_audio_path);
    let original_audio_path = Path::new(&original_audio_path); 
    let original_vtt_path = Path::new(&original_vtt_path);
    let translated_vtt_path = Path::new(&translated_vtt_path);
    
    // Call the merge_files function with the final output path
    let result = merge::merge_files(
        video_path,
        translated_audio_path,
        original_audio_path,
        original_vtt_path,
        translated_vtt_path,
        &final_output_path,
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
        output_dir,
    })
}

async fn process_steps(
    steps: Vec<Step>,
    output_path: PathBuf,
    window: tauri::Window,
) -> Result<(), Box<dyn std::error::Error>> {
    for step in steps {
        match step {
            Step::Download { url } => {
                info!("Step 1: Downloading video");
                let download_result = match download_video(window.clone(), url.clone(), output_path.to_string_lossy().to_string()).await {
                    Ok(result) => {
                        info!("Download completed successfully");
                        result
                    }
                    Err(e) => {
                        error!("Download failed: {}", e);
                        return Err(Box::new(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("Download failed: {}", e),
                        )));
                    }
                };
            }
            Step::Transcribe => {
                // TODO: Implement transcribe step
            }
            Step::Translate => {
                // TODO: Implement translate step
            }
            Step::GenerateSpeech => {
                // TODO: Implement speech generation step
            }
            Step::Merge => {
                // TODO: Implement merge step
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn cleanup_temp_files(final_video_path: String, output_dir: String) -> Result<(), String> {
    info!("Starting cleanup with final_video_path: {} and output_dir: {}", final_video_path, output_dir);

    // Убедимся что output_dir существует и является директорией
    let cleanup_dir = std::path::Path::new(&output_dir);
    if !cleanup_dir.exists() || !cleanup_dir.is_dir() {
        return Err(format!("Output directory does not exist or is not a directory: {}", output_dir));
    }

    // Get the filename from the final video path
    let final_video_name = std::path::Path::new(&final_video_path)
        .file_name()
        .ok_or("Failed to get video filename")?
        .to_str()
        .ok_or("Invalid video filename")?;

    // Get the base filename (without extension and language suffix) from the final video
    let base_filename = std::path::Path::new(&final_video_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| {
            // Remove language suffix if present (e.g., "_ru" from "video_ru.mp4")
            if let Some(pos) = s.rfind('_') {
                &s[..pos]
            } else {
                s
            }
        })
        .unwrap_or("");

    info!("Base filename for cleanup: {}", base_filename);
    info!("Cleaning up in directory: {}", cleanup_dir.display());

    // Remove the entire videonova_temp directory
    let temp_dir = cleanup_dir.join("videonova_temp");
    if temp_dir.exists() && temp_dir.is_dir() {
        info!("Removing temporary directory: {}", temp_dir.display());
        if let Err(e) = tokio::fs::remove_dir_all(&temp_dir).await {
            warn!("Failed to remove temporary directory {}: {}", temp_dir.display(), e);
        } else {
            info!("Successfully removed temporary directory: {}", temp_dir.display());
        }
    }

    Ok(())
}

/// Проверяет доступность YouTube из текущего местоположения
/// 
/// Эта функция выполняет HTTP-запрос к YouTube и анализирует ответ.
/// Возвращает true, если YouTube доступен, и false, если он заблокирован.
#[tauri::command]
pub async fn check_youtube_availability() -> Result<bool, String> {
    info!("Checking YouTube availability...");
    
    // Создаем HTTP-клиент с увеличенным таймаутом и параметрами для определения проблем
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    
    // Используем только прямой URL YouTube
    let endpoint = "https://www.youtube.com/";
    
    info!("Checking YouTube endpoint: {}", endpoint);
    match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.get(endpoint).send()
    ).await {
        Ok(Ok(response)) => {
            let status = response.status();
            info!("YouTube response status: {}", status);
            
            if status.is_success() {
                info!("YouTube is accessible");
                return Ok(true);
            }
            
            // Получаем детали ответа для анализа
            match response.text().await {
                Ok(text) => {
                    // Проверяем на наличие признаков блокировки
                    if text.contains("unavailable in your country") || 
                       text.contains("доступ ограничен") ||
                       text.contains("access denied") {
                        info!("YouTube appears to be blocked: {}", text);
                        return Ok(false);
                    }
                    
                    // Если нет явных признаков блокировки, но статус не успешный, предполагаем проблемы с доступом
                    info!("YouTube returned unsuccessful status but no explicit block indicators");
                    return Ok(false);
                },
                Err(e) => {
                    warn!("Failed to read YouTube response body: {}", e);
                    return Ok(false);
                }
            }
        },
        Ok(Err(e)) => {
            warn!("YouTube request failed: {}", e);
            
            // Анализируем тип ошибки
            if e.is_connect() || e.is_timeout() {
                // Проблемы с соединением часто указывают на блокировку
                warn!("Connection problems suggest YouTube might be blocked");
            }
            return Ok(false);
        },
        Err(_) => {
            warn!("YouTube request timed out");
            // Тайм-аут может указывать на блокировку
            return Ok(false);
        }
    }
}

/// Проверяет доступность OpenAI из текущего местоположения
/// 
/// Эта функция выполняет HTTP-запрос к ChatGPT и API OpenAI и анализирует ответ.
/// Не требует ключа API, просто проверяет доступность сервиса.
/// Возвращает true, если OpenAI доступен, и false, если он заблокирован.
#[tauri::command]
pub async fn check_openai_availability() -> Result<bool, String> {
    info!("Checking OpenAI availability...");
    
    // Создаем HTTP-клиент с увеличенным таймаутом
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    
    // Проверяем основной эндпоинт - ChatGPT
    let endpoint = "https://chatgpt.com/?hints=search";
    
    info!("Checking OpenAI endpoint: {}", endpoint);
    match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.get(endpoint).send()
    ).await {
        Ok(Ok(response)) => {
            let status = response.status();
            info!("OpenAI response status: {}", status);
            
            if status.is_success() {
                info!("OpenAI is accessible via ChatGPT");
                return Ok(true);
            }
            
            // Получаем детали ответа для анализа
            match response.text().await {
                Ok(text) => {
                    // Проверяем на наличие признаков блокировки региона
                    if text.contains("not available in your country") || 
                       text.contains("регион не поддерживается") ||
                       text.contains("service is not available") {
                        info!("OpenAI appears to be blocked for your region (ChatGPT check): {}", text);
                        // Fall through to API check before concluding service is blocked
                    } else {
                        // Если нет явных признаков блокировки, но статус не успешный, продолжаем проверку
                        info!("ChatGPT returned unsuccessful status but no explicit block indicators");
                    }
                },
                Err(e) => {
                    warn!("Failed to read ChatGPT response body: {}", e);
                }
            }
        },
        Ok(Err(e)) => {
            warn!("ChatGPT request failed: {}", e);
            
            // Анализируем тип ошибки
            if e.is_connect() || e.is_timeout() {
                // Проблемы с соединением часто указывают на блокировку
                warn!("Connection problems suggest ChatGPT might be blocked");
            }
            // Continue to API check, don't return early
        },
        Err(_) => {
            warn!("ChatGPT request timed out");
            // Continue to API check, don't return early
        }
    }

    // Проверяем дополнительно основной API эндпоинт OpenAI
    let api_endpoint = "https://api.openai.com/v1";
    info!("Checking additional OpenAI API endpoint: {}", api_endpoint);
    
    match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.get(api_endpoint).send()
    ).await {
        Ok(Ok(response)) => {
            let status = response.status();
            info!("OpenAI API response status: {}", status);
            
            // API возвращает 404 для неавторизованных запросов к корневому пути - это нормально
            // и означает, что API доступен
            if status.is_success() || status.as_u16() == 404 || status.as_u16() == 401 {
                info!("OpenAI API is accessible (returned expected status code)");
                return Ok(true);
            }
            
            // Получаем детали ответа для анализа
            match response.text().await {
                Ok(text) => {
                    info!("Analyzing OpenAI API response: {}", text);
                    
                    // Проверяем на наличие специфичной ошибки блокировки региона в формате JSON
                    if text.contains("unsupported_country_region_territory") || 
                       text.contains("Country, region, or territory not supported") {
                        info!("OpenAI API explicitly reports region blocking: {}", text);
                        return Ok(false);
                    }
                    
                    // Проверяем на наличие общих признаков блокировки региона
                    if text.contains("not available in your country") || 
                       text.contains("регион не поддерживается") ||
                       text.contains("service is not available") {
                        info!("OpenAI API appears to be blocked for your region: {}", text);
                        return Ok(false);
                    }
                    
                    // Если получен другой статус но без явных признаков блокировки,
                    // то считаем услугу доступной (возможно, просто требуется авторизация)
                    info!("OpenAI API returned non-success status but without block indicators");
                    return Ok(true);
                },
                Err(e) => {
                    warn!("Failed to read OpenAI API response body: {}", e);
                    // Если не смогли прочитать ответ, пробуем сделать вывод по симптомам
                    if status.as_u16() == 403 {
                        warn!("API returned 403 Forbidden - likely region blocked");
                        return Ok(false);
                    }
                    // В случае других ошибок при чтении, предполагаем, что сервис может быть доступен
                    return Ok(true);
                }
            }
        },
        Ok(Err(e)) => {
            warn!("OpenAI API request failed: {}", e);
            
            // Анализируем тип ошибки
            if e.is_connect() || e.is_timeout() {
                // Проблемы с соединением часто указывают на блокировку
                warn!("Connection problems suggest OpenAI API might be blocked");
                return Ok(false);
            }
            
            // Другие типы ошибок могут быть связаны с временными проблемами, не обязательно блокировкой
            return Ok(false);
        },
        Err(_) => {
            warn!("OpenAI API request timed out");
            // Тайм-аут может указывать на блокировку
            return Ok(false);
        }
    }
}

/// Структура для передачи результатов проверки доступности сервисов
#[derive(Serialize)]
pub struct ServiceAvailabilityResult {
    pub youtube_available: bool,
    pub openai_available: bool,
    pub vpn_required: bool,
    pub message: String,
    pub is_retry: bool,
}

/// Проверяет доступность всех нужных сервисов и возвращает результат
/// с рекомендациями для пользователя о необходимости VPN
/// Не показывает диалоговые окна - UI сам отобразит информацию пользователю
#[tauri::command]
pub async fn check_services_availability(window: tauri::WebviewWindow, is_retry: Option<bool>) -> Result<ServiceAvailabilityResult, String> {
    // Определяем, является ли эта проверка повторной
    let is_retry = is_retry.unwrap_or(false);
    
    // Отправляем событие о начале проверки
    let _ = window.emit("services-check-started", json!({
        "is_retry": is_retry
    }));
    
    info!("Checking availability of required services... (retry: {})", is_retry);
    
    // Создаем токен отмены, чтобы иметь возможность ограничить максимальное время всей проверки
    let cancellation_token = tokio_util::sync::CancellationToken::new();
    let ct_clone = cancellation_token.clone();
    
    // Устанавливаем таймаут для всей операции проверки в 5 секунд
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        ct_clone.cancel();
    });
    
    // Проверяем YouTube с защитой от отмены
    let _ = window.emit("checking-youtube", ());
    let youtube_available = match tokio::select! {
        result = check_youtube_availability() => result,
        _ = cancellation_token.cancelled() => {
            warn!("YouTube availability check was cancelled due to timeout");
            Ok(false) // Если проверка была отменена из-за таймаута, считаем сервис недоступным
        }
    } {
        Ok(available) => {
            info!("YouTube availability check result: {}", available);
            let _ = window.emit("youtube-check-complete", available);
            available
        },
        Err(e) => {
            error!("Error checking YouTube availability: {}", e);
            let _ = window.emit("youtube-check-error", &e);
            // В случае ошибки проверки предполагаем, что сервис может быть недоступен
            false
        }
    };
    
    // Проверяем OpenAI с защитой от отмены
    let _ = window.emit("checking-openai", ());
    let openai_available = match tokio::select! {
        result = check_openai_availability() => result,
        _ = cancellation_token.cancelled() => {
            warn!("OpenAI availability check was cancelled due to timeout");
            Ok(false) // Если проверка была отменена из-за таймаута, считаем сервис недоступным
        }
    } {
        Ok(available) => {
            info!("OpenAI availability check result: {}", available);
            let _ = window.emit("openai-check-complete", available);
            available
        },
        Err(e) => {
            error!("Error checking OpenAI availability: {}", e);
            let _ = window.emit("openai-check-error", &e);
            // В случае ошибки проверки предполагаем, что сервис может быть недоступен
            false
        }
    };
    
    // Определяем, нужен ли VPN
    let vpn_required = !youtube_available || !openai_available;
    
    // Формируем сообщение для пользователя в зависимости от результата и типа проверки
    let message = if vpn_required {
        let mut blocked_services = Vec::new();
        if !youtube_available {
            blocked_services.push("YouTube");
        }
        if !openai_available {
            blocked_services.push("OpenAI");
        }
        
        if is_retry {
            format!(
                "Сервисы все еще недоступны: {}. \
                Пожалуйста, убедитесь, что VPN включен и корректно настроен.",
                blocked_services.join(", ")
            )
        } else {
            format!(
                "Для корректной работы приложения требуется VPN"
            )
        }
    } else {
        if is_retry {
            "Все необходимые сервисы теперь доступны! VPN работает корректно.".to_string()
        } else {
            "Все необходимые сервисы доступны. VPN не требуется.".to_string()
        }
    };
    
    // Отправляем событие о завершении проверки
    let _ = window.emit("services-check-completed", json!({
        "vpn_required": vpn_required,
        "is_retry": is_retry,
        "youtube_available": youtube_available,
        "openai_available": openai_available,
        "message": message
    }));
    
    Ok(ServiceAvailabilityResult {
        youtube_available,
        openai_available,
        vpn_required,
        message,
        is_retry,
    })
}

/// Open a file using the system's default program
#[tauri::command]
pub async fn open_file(app: tauri::AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| e.to_string())
}
