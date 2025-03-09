use log::{error, info, warn};
use reqwest;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::Emitter;
use tokio::sync::mpsc;
use serde_json::json;
use futures::FutureExt;
use tts_sync::{
    progress::{DefaultProgressReporter, ProgressObserver, ProgressInfo, ProgressReporter},
    synchronize_tts_with_progress,
};

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

impl ProgressObserver for TauriProgressObserver {
    fn on_progress_update(&self, progress: ProgressInfo) {
        if let Err(e) = self.window.emit("tts-progress", json!({
            "step": progress.step,
            "step_progress": progress.step_progress,
            "total_progress": progress.total_progress,
            "details": progress.details
        })) {
            error!("Failed to emit TTS progress: {}", e);
        } else {
            info!("TTS progress emitted: step={}, progress={:.1}%, total={:.1}%",
                progress.step, progress.step_progress, progress.total_progress);
        }
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
    reporter: Box<dyn ProgressReporter>,
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
    
    // Create a TTS config with the API key
    let config = tts_sync::config::TtsSyncConfig {
        openai_api_key: api_key.to_string(),
        ..tts_sync::config::TtsSyncConfig::default()
    };
    
    // Create a TTS sync instance with the reporter
    let tts_sync = tts_sync::TtsSync::with_progress_reporter(config, reporter);
    
    // Use a detailed try/catch approach to identify where issues occur
    info!("About to start TTS sync process - this is where we often get stuck");
    
    // Wrap in a timeout to prevent hanging indefinitely
    let process_future = async {
        let result = tts_sync.process(
            video_path,
            audio_path,
            original_vtt_path,
            translated_vtt_path,
            output_path,
        ).await;
        
        match result {
            Ok(output_file) => {
                info!("TTS process completed successfully!");
                Ok(output_file)
            },
            Err(e) => {
                error!("TTS process returned an error: {:?}", e);
                Err(format!("TTS error: {:?}", e))
            }
        }
    };
    
    // Add a timeout
    match tokio::time::timeout(
        std::time::Duration::from_secs(30), // 30 second timeout for testing
        process_future
    ).await {
        Ok(result) => result,
        Err(_) => {
            error!("TTS process timed out after 30 seconds");
            Err("TTS process timed out - likely stuck in API request or processing".to_string())
        }
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
    
    // Create progress reporter and observer
    let mut reporter = DefaultProgressReporter::new();
    let observer = TauriProgressObserver::new(window.clone());
    reporter.add_observer(Box::new(observer));
    
    // Use our enhanced TTS function with detailed logging
    match enhanced_tts_with_logging(
        &video_path,
        &audio_path,
        &original_vtt_path,
        &translated_vtt_path,
        &output_path,
        &api_key,
        Box::new(reporter),
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
    
    // Создаем отдельную директорию для финального результата
    let final_dir = PathBuf::from(&output_path).join("final");
    tokio::fs::create_dir_all(&final_dir)
        .await
        .map_err(|e| format!("Failed to create final directory: {}", e))?;
    
    let final_output = final_dir.join("final_output.mp4");
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
