use tauri::{Window, Manager, Emitter};
use log::{error, info, warn};
use serde::Serialize;
use tauri_plugin_store::StoreExt;
use std::path::PathBuf;
use tokio::sync::mpsc;
use serde_json::json;
use crate::services::tts::fishspeech;
use crate::config::tts::TtsConfig;
use crate::services::tts::tts::ProgressUpdate;
use crate::models::tts::{SpeechGenerationRequest, SpeechGenerationResult};
use crate::errors::AppResult;

use crate::services;
use crate::commands::video_commands::{get_video_info, download_video};
use crate::commands::transcription_commands::transcribe_audio;
use crate::commands::translation_commands::translate_vtt;

/// Сегмент текста для генерации речи
#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Serialize)]
pub struct TTSResult {
    pub audio_path: String,
    pub duration: f64,
}

#[derive(Serialize)]
pub struct ProcessVideoResult {
    pub video_path: String,
    pub audio_path: String,
    pub transcription_path: String,
    pub translation_path: String,
    pub tts_path: String,
    pub final_path: String,
    pub merged_path: String,
}

#[derive(Serialize)]
pub struct MergeResult {
    pub merged_video_path: String,
    pub output_dir: String,
}

struct TauriProgressObserver {
    window: Window,
}

impl TauriProgressObserver {
    fn new(window: Window) -> Self {
        Self { window }
    }
}

/// Generate speech from translated text
#[tauri::command]
pub async fn generate_speech_v2(request: crate::models::tts::SpeechGenerationRequest) -> AppResult<crate::models::tts::SpeechGenerationResult> {
    info!("Generating speech with engine: {}", request.engine);
    
    // Get the appropriate TTS service
    let config = crate::config::AppConfig::default();
    let service = crate::services::tts::get_tts_service(&request.engine, &config)?;
    
    // Create audio processing config
    let audio_config = crate::services::tts::AudioProcessingConfig::default();
    
    // Call the service
    let tts_request = crate::models::SpeechGenerationRequest {
        text: request.text.clone(),
        output_path: std::path::PathBuf::from("/tmp/output.wav"), // Временный путь
        engine: request.engine.clone(),
        remove_vocals: false,
        adjust_pitch: 0.0,
        mix_with_instrumental: false,
        voice_to_instrumental_ratio: 0.7,
    };
    
    let result = service.generate_speech(&tts_request, &audio_config, None).await?;
    
    Ok(crate::models::tts::SpeechGenerationResult {
        audio_path: result.output_path.to_string_lossy().to_string(),
        duration: result.duration as f64,
        tokens_used: None,
    })
}

/// Process a video through all steps: download, transcribe, translate, and generate speech
#[tauri::command]
pub async fn process_video(
    url: String,
    output_path: String,
    target_language: String,
    target_language_name: String,
    source_language_code: String,
    source_language_name: String,
    api_key: String,
    window: Window,
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
    let app_handle = window.app_handle();
    let url_regex = regex::Regex::new(r"^https?://").unwrap();
    let (video_path, audio_path) = if url_regex.is_match(&url) {
        info!("Video path is a URL, downloading video first");
        
        // Создаем директорию для скачивания
        let output_dir = output_path.clone();
        tokio::fs::create_dir_all(&output_dir).await.map_err(|e| format!("Failed to create output directory: {}", e))?;
        
        // Скачиваем видео
        let download_result = match download_video(app_handle.clone(), url.clone(), output_dir, window.clone()).await {
            Ok(result) => result,
            Err(e) => return Err(format!("Failed to download video: {}", e)),
        };
        (download_result.video_path, download_result.audio_path)
    } else {
        (url, String::new()) // If local file, no audio path
    };

    // Step 2: Transcribe audio
    info!("Step 2: Transcribing audio");
    let transcription_result = match transcribe_audio(
        video_path.clone(),
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

    // Small pause after translation and file check
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Check all necessary files before starting TTS
    for path_str in [
        &video_path,
        &transcription_result.vtt_path,
        &translation_result.translated_vtt_path,
    ] {
        let path = std::path::Path::new(path_str);
        if !services::common::check_file_exists_and_valid(path).await {
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
    let original_filename = std::path::Path::new(&video_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "video".to_string());
    
    // Save to tts subdirectory with .wav extension
    let tts_output = tts_dir.join(format!("{}_tts.wav", original_filename));
    info!("TTS output will be saved to: {}", tts_output.display());

    let tts_result = generate_speech_for_segments(
        video_path.clone(),
        video_path.clone(), // Используем video_path как audio_path
        output_path.clone(),
        window.clone(),
    ).await?;

    // We need to determine source language code from transcription
    let merge_result = merge_video(
        video_path.clone(),
        video_path.clone(), // video_path
        tts_result.audio_path.clone(), // Use the TTS result as the translated audio
        video_path.clone(), // audio_path (используем video_path как audio_path)
        transcription_result.vtt_path.clone(),
        translation_result.translated_vtt_path.clone(),
        output_path.clone(), // Use the user-selected output directory directly
        source_language_code.clone(),
        target_language.clone(),
        source_language_name.clone(),
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
    if let Err(e) = crate::commands::utility_commands::cleanup_temp_files(
        merge_result.merged_video_path.clone(),
        output_path.clone()
    ).await {
        warn!("Failed to cleanup temporary files: {}", e);
        // Don't return error here, as the main process was successful
    }

    Ok(ProcessVideoResult {
        video_path: video_path,
        audio_path: audio_path,
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
    window: Window,
) -> Result<MergeResult, String> {
    info!("Starting video merging process");
    
    let (progress_tx, mut progress_rx) = mpsc::channel::<services::merge::MergeProgress>(32);
    
    // Clone window for progress updates
    let window_clone = window.clone();
    
    // Spawn a task to forward progress updates to the frontend
    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            let (status, progress_value) = match &progress {
                services::merge::MergeProgress::Started => ("started", 0.0),
                services::merge::MergeProgress::Progress(p) => ("progress", *p),
                services::merge::MergeProgress::Completed => ("completed", 1.0),
                services::merge::MergeProgress::Error(err) => ("error", 0.0),
            };
            
            let _ = window_clone.emit("merge-progress", json!({
                "status": status,
                "progress": progress_value,
                // Add additional fields to ensure compatibility with UI
                "step": "Video Merging",
                "step_progress": progress_value,
                "total_progress": progress_value
            }));
        }
    });

    // Get original video filename without extension
    let video_filename = std::path::Path::new(&video_path)
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
    let video_path = std::path::Path::new(&video_path);
    let translated_audio_path = std::path::Path::new(&translated_audio_path);
    let original_audio_path = std::path::Path::new(&original_audio_path); 
    let original_vtt_path = std::path::Path::new(&original_vtt_path);
    let translated_vtt_path = std::path::Path::new(&translated_vtt_path);
    
    // Call the merge_files function with the final output path
    let result = services::merge::merge_files(
        video_path,
        translated_audio_path,
        &final_output_path,
        Some(progress_tx)
    ).await.map_err(|e| format!("Failed to merge files: {}", e))?;
    
    info!("Merging completed successfully");
    info!("  Merged video path: {}", result.display());
    
    Ok(MergeResult {
        merged_video_path: result.to_string_lossy().to_string(),
        output_dir,
    })
}

/// Generate speech using OpenAI TTS API
async fn generate_speech_with_openai(
    video_path: String,
    audio_path: String,
    original_vtt_path: String,
    translated_vtt_path: String,
    output_path: String,
    api_key: String,
    window: Window,
) -> Result<TTSResult, String> {
    // Get app_handle from window
    let app_handle = window.app_handle();
    
    // Load TTS settings to get the selected voice
    let tts_config = match app_handle.store(".settings.dat") {
        Ok(store) => {
            // Get value from store
            match store.get("tts_config") {
                Some(value) => {
                    match serde_json::from_value::<crate::config::tts::TtsConfig>(value.clone()) {
                        Ok(config) => config,
                        Err(e) => {
                            warn!("Failed to parse TTS config: {}", e);
                            crate::config::tts::TtsConfig::default()
                        }
                    }
                },
                None => {
                    // If no configuration, use default values
                    crate::config::tts::TtsConfig::default()
                }
            }
        },
        Err(e) => {
            warn!("Failed to access store: {}", e);
            crate::config::tts::TtsConfig::default()
        }
    };
    
    // Use voice from settings or default value
    let voice = tts_config.openai_voice.unwrap_or_else(|| {
        warn!("No voice specified in TTS config, using default 'alloy'");
        "alloy".to_string()
    });
    
    info!("Using OpenAI voice: {}", voice);
    
    // Validate input files
    for (path, desc) in [
        (&video_path, "video"),
        (&audio_path, "audio"),
        (&original_vtt_path, "original subtitles"),
        (&translated_vtt_path, "translated subtitles"),
    ] {
        if !services::common::check_file_exists_and_valid(std::path::Path::new(path)).await {
            error!("File not found or invalid: {} ({})", path, desc);
            return Err(format!("Required {} file not found or invalid: {}", desc, path));
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
                duration: 0.0,
            })
        },
        Err(e) => {
            error!("TTS generation failed: {}", e);
            Err(e)
        }
    }
}

/// Generate speech using Fish Speech engine
async fn generate_speech_with_fish_speech(
    video_path: String,
    audio_path: String,
    original_vtt_path: String,
    translated_vtt_path: String,
    output_path: String,
    source_language_code: String,
    target_language: String,
    source_language_name: String,
    target_language_name: String,
    window: Window,
) -> Result<TTSResult, String> {
    // Get app_handle from window
    let app_handle = window.app_handle();
    
    // Validate input files
    for (path, desc) in [
        (&video_path, "video"),
        (&audio_path, "audio"),
        (&original_vtt_path, "original subtitles"),
        (&translated_vtt_path, "translated subtitles"),
    ] {
        if !services::common::check_file_exists_and_valid(std::path::Path::new(path)).await {
            error!("File not found or invalid: {} ({})", path, desc);
            return Err(format!("Required {} file not found or invalid: {}", desc, path));
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
    
    info!("Fish Speech TTS output will be saved to: {}", output_path);
    
    // Initialize Fish Speech
    fishspeech::initialize().await.map_err(|e| format!("Failed to initialize Fish Speech: {}", e))?;
    
    // Get configuration
    let fish_config = fishspeech::get_config().map_err(|e| format!("Failed to get Fish Speech config: {}", e))?;
    
    // Get list of voices
    let voices = fishspeech::list_voices().await.map_err(|e| format!("Failed to list Fish Speech voices: {}", e))?;
    if voices.is_empty() {
        return Err("No voices available in Fish Speech".to_string());
    }
    
    // Load TTS settings
    let tts_config = match app_handle.store(".settings.dat") {
        Ok(store) => {
            // Get value from store
            match store.get("tts_config") {
                Some(value) => {
                    match serde_json::from_value::<crate::config::tts::TtsConfig>(value.clone()) {
                        Ok(config) => config,
                        Err(e) => {
                            warn!("Failed to parse TTS config: {}", e);
                            return Err(format!("Failed to parse TTS config: {}", e));
                        }
                    }
                },
                None => {
                    // If no configuration, return error
                    return Err("TTS configuration not found".to_string());
                }
            }
        },
        Err(e) => {
            error!("Failed to access store: {}", e);
            return Err(format!("Failed to access store: {}", e));
        }
    };
    
    // Select voice from settings or first available
    let voice_id = if let Some(voice_id) = &tts_config.fish_speech_voice_id {
        voice_id.clone()
    } else if let Some(default_voice) = voices.first() {
        default_voice.clone()
    } else {
        return Err("No default voice available".to_string());
    };
    
    // Parse VTT file to get segments
    let vtt_content = tokio::fs::read_to_string(&translated_vtt_path)
        .await
        .map_err(|e| format!("Failed to read translated VTT file: {}", e))?;
    
    let segments = services::tts::vtt::parse_vtt(std::path::Path::new(&translated_vtt_path))
        .map_err(|e| format!("Failed to parse VTT file: {}", e))?;
    
    let total_segments = segments.len();
    info!("Processing {} segments from VTT file", total_segments);
    
    let mut audio_files = Vec::new();
    
    // Create channel for progress
    let (progress_tx, mut progress_rx) = mpsc::channel(8);
    
    // Create separate channel for merge_files
    let (merge_progress_tx, _) = mpsc::channel::<services::merge::MergeProgress>(8);
    
    // Start progress monitoring task
    let progress_window = window.clone();
    let progress_task = tokio::spawn(async move {
        while let Some(update) = progress_rx.recv().await {
            match update {
                services::tts::ProgressUpdate::TTSGeneration { current, total } => {
                    let progress = (current as f32 / total as f32) * 100.0;
                    let payload = json!({
                        "status": "Generating TTS",
                        "progress": progress,
                        "message": format!("Generating speech for segment {}/{}", current, total),
                        "step": "tts-generation",
                        "current": current as i32,
                        "total": total as i32,
                    });
                    let _ = progress_window.emit("tts-progress", payload);
                },
                _ => { /* Ignore other update types */ }
            }
        }
    });
    
    // Generate TTS for each segment
    for (i, segment) in segments.iter().enumerate() {
        // Send progress update
        let _ = progress_tx.send(services::tts::ProgressUpdate::TTSGeneration { 
            current: i, 
            total: total_segments 
        }).await;
        
        // Create Fish Speech request
        let request = fishspeech::TtsRequest {
            text: segment.text.clone(),
            voice_id: voice_id.clone(),
            format: fishspeech::SpeechFormat::Wav,
            rate: 1.0,
            stream: false,
        };
        
        // Generate speech
        let result = fishspeech::generate_speech(request).await
            .map_err(|e| format!("Failed to generate speech for segment {}: {}", i, e))?;
        
        // Add file to list
        audio_files.push((result.audio_path.to_string_lossy().to_string(), segment.start, segment.end));
    }
    
    // Close progress channel
    drop(progress_tx);
    
    // Merge audio files and synchronize with original audio
    info!("Merging {} audio files and synchronizing with original audio", audio_files.len());
    
    // Pass audio files to MergeTool
    let merge_result = services::merge::merge_files(
        std::path::Path::new(&video_path),
        std::path::Path::new(&audio_path),
        std::path::Path::new(&output_path),
        Some(merge_progress_tx.clone()),
    )
    .await
    .map_err(|e| {
        error!("Merging failed: {}", e);
        format!("Merging failed: {}", e)
    })?;
    
    // Wait for progress monitoring task to complete
    let _ = progress_task.await;
    
    info!("Fish Speech TTS generation completed successfully");
    Ok(TTSResult {
        audio_path: merge_result.to_string_lossy().to_string(),
        duration: 0.0,
    })
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
    std::thread::spawn(move || {
        // Create a runtime for the thread
        match tokio::runtime::Runtime::new() {
            Ok(rt) => {
                // Run the TTS synchronization in the runtime
                rt.block_on(async {
                    // Create a task to handle progress updates
                    let progress_window = window_clone.clone();
                    let progress_state = std::sync::Arc::new(std::sync::Mutex::new(0.0f32));
                    
                    // Spawn a task to handle progress updates from the TTS library
                    let progress_task = tokio::spawn(async move {
                        // Add a tracked highest progress value to prevent decreases
                        let mut highest_progress = 0.0f32;
                        
                        while let Some(update) = progress_rx.recv().await {
                            use crate::services::tts::tts::ProgressUpdate;
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
                                ProgressUpdate::Normalizing { using_original: _ } => (95.0, "Нормализация громкости".to_string(), None, None),
                                ProgressUpdate::Encoding => (98.0, "Сохранение результата".to_string(), None, None),
                                ProgressUpdate::Finished => (100.0, "TTS готов".to_string(), None, None),
                                ProgressUpdate::Completed => (100.0, "TTS готов".to_string(), None, None),
                            };
                            
                            // Make sure progress is in range 0-100
                            let mut normalized_progress = progress.max(0.0).min(100.0);
                            
                            // Never decrease progress (except for new starts)
                            if normalized_progress < highest_progress && normalized_progress > 1.0 {
                                info!("Prevented progress decrease: {} -> {}", normalized_progress, highest_progress);
                                normalized_progress = highest_progress;
                            } else if normalized_progress > highest_progress {
                                highest_progress = normalized_progress;
                            }
                            
                            let should_send = {
                                // Get access to previous progress
                                let mut previous_progress = match progress_state.lock() {
                                    Ok(guard) => guard,
                                    Err(_) => return, // In case of error just exit
                                };
                                
                                // Only send updates if progress has increased and exceeds a threshold, or for important status changes
                                let should_update = 
                                    (normalized_progress > *previous_progress && normalized_progress - *previous_progress >= 0.5) || 
                                    normalized_progress == 0.0 || normalized_progress >= 99.9 ||
                                    status.contains("готов");
                                
                                // Update previous progress value
                                if should_update {
                                    *previous_progress = normalized_progress;
                                }
                                
                                should_update
                            };
                            
                            // Send updates only if needed
                            if should_send {
                                // Create progress object
                                let progress_json = json!({
                                    "step": "TTS Generation",
                                    "step_progress": normalized_progress,
                                    "total_progress": normalized_progress,
                                    "details": status,
                                    "current_segment": current,
                                    "total_segments": total,
                                    "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64,
                                    "status": status,  // explicitly add status for UI
                                    "progress": normalized_progress  // explicitly add progress field for compatibility with progress interface
                                });
                                
                                // Always log progress for debugging
                                info!("TTS progress: {:.1}%, status={}", normalized_progress, status);
                                
                                // Send event
                                if let Err(e) = progress_window.emit("tts-progress", progress_json.clone()) {
                                    error!("Failed to emit TTS progress: {}", e);
                                }
                            }
                        }
                    });
                    
                    // Set up the configuration for our TTS library
                    let vtt_path = std::path::Path::new(&translated_vtt_path_clone);
                    let output_wav_path = std::path::Path::new(&output_path_clone);
                    let original_audio = Some(std::path::Path::new(&audio_path_clone));
                    
                    // Create TTS configuration with sensible defaults
                    let tts_config = crate::config::tts::TtsConfig {
                        engine: crate::config::tts::TtsEngine::OpenAI,
                        openai_voice: Some("alloy".to_string()),
                        fish_speech_voice_id: None,
                        fish_speech_use_gpu: true,
                    };
                    
                    // Create audio processing configuration with sensible defaults
                    let audio_config = services::tts::AudioProcessingConfig {
                        window_size: 4096,
                        hop_size: 1024,
                        target_peak_level: 0.8,
                        voice_to_instrumental_ratio: 0.6,
                    };
                    
                    // Создаем канал для отправки прогресса
                    let (progress_tx, progress_rx) = mpsc::channel(32);
                    
                    // Создаем отдельный канал для отправки прогресса в TTS модуль
                    let (tts_progress_tx, mut tts_progress_rx) = mpsc::channel(32);
                    
                    // Запускаем задачу для преобразования прогресса из TTS в общий формат
                    let progress_conversion_task = tokio::spawn(async move {
                        while let Some(update) = tts_progress_rx.recv().await {
                            // Конвертируем ProgressUpdate из tts в общий формат
                            let converted_update = match update {
                                services::tts::tts::ProgressUpdate::Started => services::tts::ProgressUpdate::Started,
                                services::tts::tts::ProgressUpdate::TTSGeneration { current, total } => 
                                    services::tts::ProgressUpdate::TTSGeneration { current, total },
                                _ => services::tts::ProgressUpdate::ProcessingAudio(0.5),
                            };
                            
                            if let Err(e) = progress_tx.send(converted_update).await {
                                error!("Failed to send converted progress update: {}", e);
                            }
                        }
                    });
                    
                    // Create the sync configuration
                    let sync_config = services::tts::common::synchronizer::SyncConfig {
                        api_key: &api_key_clone,
                        vtt_path,
                        output_wav: output_wav_path,
                        original_audio_path: original_audio,
                        progress_sender: Some(tts_progress_tx),
                        tts_config,
                        audio_config,
                    };
                    
                    // Run the TTS synchronization
                    info!("Starting TTS synchronization with video duration: {:.2}s", video_duration);
                    match services::tts::common::synchronizer::process_sync(sync_config).await {
                        Ok(()) => {
                            info!("TTS process completed successfully!");
                            info!("Generated TTS output file: {}", output_path_clone);
                            
                            // Verify the generated file exists and has content
                            match tokio::fs::metadata(&output_path_clone).await {
                                Ok(metadata) => {
                                    let file_size = metadata.len();
                                    info!("Generated file size: {} bytes", file_size);
                                    
                                    if file_size < 1000 {  // If file is less than 1KB, it's probably empty or corrupt
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

/// Helper function to get video duration
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

/// Генерирует речь для сегментов
async fn generate_speech_for_segments(
    video_path: String,
    audio_path: String,
    output_path: String,
    window: Window,
) -> Result<TTSResult, String> {
    info!("Generating speech for segments");
    
    // Заглушка для функции
    Ok(TTSResult {
        audio_path: output_path,
        duration: 0.0,
    })
} 