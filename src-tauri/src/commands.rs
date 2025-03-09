use anyhow::anyhow;
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

use crate::utils::common::{check_file_exists_and_valid, sanitize_filename};
use crate::utils::merge;
use crate::utils::transcribe;
use crate::utils::translate;
use crate::utils::tts;
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

#[derive(Serialize, Debug)]
pub struct MergeResult {
    output_path: String,
    output_dir: String,
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
        match youtube::download_video(&url, &output_dir, Some(tx)).await {
            Ok(result) => {
                // Verify downloaded files
                let video_exists = tokio::fs::metadata(&result.video_path).await.is_ok();
                let audio_exists = tokio::fs::metadata(&result.audio_path).await.is_ok();

                if !video_exists || !audio_exists {
                    error!("Download verification failed:");
                    error!("  Video file exists: {}", video_exists);
                    error!("  Audio file exists: {}", audio_exists);

                    if current_attempt < MAX_RETRIES {
                        warn!("Retrying download...");
                        // Небольшая пауза перед следующей попыткой
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        continue;
                    } else {
                        return Err(format!(
                            "Failed to download after {} attempts. Files missing.",
                            MAX_RETRIES
                        ));
                    }
                }

                // Check file sizes
                let video_size = tokio::fs::metadata(&result.video_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);
                let audio_size = tokio::fs::metadata(&result.audio_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);

                if video_size == 0 || audio_size == 0 {
                    error!("Downloaded files are empty:");
                    error!("  Video size: {} bytes", video_size);
                    error!("  Audio size: {} bytes", audio_size);

                    if current_attempt < MAX_RETRIES {
                        warn!("Retrying download...");
                        // Удаляем пустые файлы перед повторной попыткой
                        if let Err(e) = tokio::fs::remove_file(&result.video_path).await {
                            warn!("Failed to remove empty video file: {}", e);
                        }
                        if let Err(e) = tokio::fs::remove_file(&result.audio_path).await {
                            warn!("Failed to remove empty audio file: {}", e);
                        }
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        continue;
                    } else {
                        return Err(format!(
                            "Failed to download after {} attempts. Files are empty.",
                            MAX_RETRIES
                        ));
                    }
                }

                info!(
                    "Download completed successfully on attempt {}",
                    current_attempt
                );
                info!(
                    "  Video path: {} ({} bytes)",
                    result.video_path.display(),
                    video_size
                );
                info!(
                    "  Audio path: {} ({} bytes)",
                    result.audio_path.display(),
                    audio_size
                );

                // Wait for progress monitoring to complete
                let _ = progress_handle.await;

                // Создаем результат для возврата
                let download_result = DownloadResult {
                    video_path: result.video_path.to_string_lossy().to_string(),
                    audio_path: result.audio_path.to_string_lossy().to_string(),
                };

                // Отправляем событие download-complete с результатом
                if let Err(e) = window.emit("download-complete", download_result.clone()) {
                    error!("Failed to emit download-complete event: {}", e);
                }

                return Ok(download_result);
            }
            Err(e) => {
                error!("Download attempt {} failed: {}", current_attempt, e);

                if current_attempt < MAX_RETRIES {
                    warn!("Retrying download...");
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    continue;
                } else {
                    return Err(format!(
                        "Failed to download after {} attempts: {}",
                        MAX_RETRIES, e
                    ));
                }
            }
        }
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
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    Ok(response.status().is_success())
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
    // Create progress channel
    let (tx, mut rx) = mpsc::channel::<translate::TranslationProgress>(32);

    // Clone window handle for the progress monitoring task
    let progress_window = window.clone();

    // Spawn progress monitoring task
    let monitoring_task = tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            // Emit progress event to frontend
            if let Err(e) = progress_window.emit("translation-progress", progress) {
                eprintln!("Failed to emit translation progress: {}", e);
            }
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

    // Wait for monitoring task to complete
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

/// Generate speech audio from VTT subtitle file using OpenAI TTS API
#[tauri::command]
pub async fn generate_speech(
    vtt_path: String,
    output_path: String,
    api_key: String,
    voice: String,
    model: String,
    words_per_second: f64,
    base_filename: String,
    language_suffix: String,
    window: tauri::Window,
) -> Result<TTSResult, String> {
    // Create progress channel
    let (tx, mut rx) = mpsc::channel::<tts::TTSProgress>(32);

    // Clone window handle for the progress monitoring task
    let progress_window = window.clone();

    // Spawn progress monitoring task
    let monitoring_task = tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            // Emit progress event to frontend
            if let Err(e) = progress_window.emit("tts-progress", progress) {
                eprintln!("Failed to emit TTS progress: {}", e);
            }
        }
    });

    // Start TTS generation
    let vtt_file = PathBuf::from(vtt_path);
    let output_dir = PathBuf::from(output_path);

    let result_path = tts::generate_tts(
        &vtt_file,
        &output_dir,
        &api_key,
        &voice,
        &model,
        words_per_second,
        &base_filename,
        &language_suffix,
        Some(tx),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Wait for monitoring task to complete
    let _ = monitoring_task.await;

    Ok(TTSResult {
        audio_path: result_path.to_string_lossy().to_string(),
    })
}

/// Final step: merge video with translated audio and subtitles using ffmpeg
#[tauri::command]
pub async fn merge_media(
    video_path: String,
    translated_audio_path: String,
    original_audio_path: String,
    original_vtt_path: String,
    translated_vtt_path: String,
    output_path: String,
    source_language_code: String, // Add this parameter
    target_language_code: String, // Add this parameter
    window: tauri::Window,
) -> Result<MergeResult, String> {
    log::info!("=== MERGE_MEDIA COMMAND STARTED ===");
    log::info!("Input parameters received:");
    log::info!("  Video: {}", video_path);
    log::info!("  Translated Audio: {}", translated_audio_path);
    log::info!("  Original Audio: {}", original_audio_path);
    log::info!("  Original VTT: {}", original_vtt_path);
    log::info!("  Translated VTT: {}", translated_vtt_path);
    log::info!("  Output Path: {}", output_path);

    // Validate input files
    let video_file = PathBuf::from(&video_path);
    let translated_audio_file = PathBuf::from(&translated_audio_path);
    let original_audio_file = PathBuf::from(&original_audio_path);
    let original_vtt_file = PathBuf::from(&original_vtt_path);
    let translated_vtt_file = PathBuf::from(&translated_vtt_path);

    if !video_file.exists() {
        log::error!("Validation failed: Video file not found: {}", video_path);
        return Err(format!("Video file not found: {}", video_path));
    }
    if !translated_audio_file.exists() {
        log::error!(
            "Validation failed: Translated audio file not found: {}",
            translated_audio_path
        );
        return Err(format!(
            "Translated audio file not found: {}",
            translated_audio_path
        ));
    }
    if !original_audio_file.exists() {
        log::error!(
            "Validation failed: Original audio file not found: {}",
            original_audio_path
        );
        return Err(format!(
            "Original audio file not found: {}",
            original_audio_path
        ));
    }
    if !original_vtt_file.exists() {
        log::error!(
            "Validation failed: Original VTT file not found: {}",
            original_vtt_path
        );
        return Err(format!(
            "Original VTT file not found: {}",
            original_vtt_path
        ));
    }
    if !translated_vtt_file.exists() {
        log::error!(
            "Validation failed: Translated VTT file not found: {}",
            translated_vtt_path
        );
        return Err(format!(
            "Translated VTT file not found: {}",
            translated_vtt_path
        ));
    }

    // Создаем канал для мониторинга прогресса
    let (tx, mut rx) = mpsc::channel::<merge::MergeProgress>(32);
    let progress_window = window.clone();

    // Мониторинг задачи с подробным логированием прогресса
    let monitoring_task = tokio::spawn(async move {
        log::info!("Merge progress monitoring task started");
        while let Some(progress) = rx.recv().await {
            log::info!(
                "Merge progress: status={}, progress={:.1}%",
                progress.status,
                progress.progress
            );
            if let Err(e) = progress_window.emit("merge-progress", progress) {
                log::error!("Failed to emit merge progress: {}", e);
            }
        }
        log::info!("Merge progress monitoring task completed");
    });

    // Определяем директорию для вывода по output_path
    let output_dir = PathBuf::from(&output_path);

    log::info!("Output directory determined: {}", output_dir.display());

    // Дополнительное логирование: проверяем, существует ли выходная директория
    if !output_dir.exists() {
        log::warn!(
            "Output directory does not exist. Attempting to create: {}",
            output_dir.display()
        );
        if let Err(e) = tokio::fs::create_dir_all(&output_dir).await {
            log::error!("Failed to create output directory: {}", e);
            return Err(format!("Failed to create output directory: {}", e));
        }
    }

    log::info!("Calling merge::merge_files...");
    // Запускаем процесс объединения
    let result = match merge::merge_files(
        &video_file,
        &translated_audio_file,
        &original_audio_file,
        &original_vtt_file,
        &translated_vtt_file,
        &output_dir,
        &source_language_code, // Pass source language code
        &target_language_code, // Pass target language code
        Some(tx),
    )
    .await
    {
        Ok(result_path) => {
            log::info!(
                "Merge completed successfully. Output path: {}",
                result_path.display()
            );
            // Дополнительная проверка: существует ли результат и не пустой ли файл
            match tokio::fs::metadata(&result_path).await {
                Ok(metadata) => {
                    if (metadata.len() == 0) {
                        log::error!("Merged file is empty: {}", result_path.display());
                        return Err("Merged file is empty".to_string());
                    } else {
                        log::info!("Merged file size: {} bytes", metadata.len());
                    }
                }
                Err(e) => {
                    log::error!("Unable to get metadata for merged file: {}", e);
                    return Err(format!("Unable to verify merged file: {}", e));
                }
            }
            // Ожидаем завершения мониторинга прогресса
            let _ = monitoring_task.await;
            // Отправляем событие merge-complete при успехе
            if let Err(e) = window.emit("merge-complete", true) {
                log::error!("Failed to emit merge-complete event: {}", e);
            }
            Ok(MergeResult {
                output_path: result_path.to_string_lossy().to_string(),
                output_dir: output_dir.to_string_lossy().to_string(),
            })
        }
        Err(e) => {
            log::error!("Merge failed: {}", e);
            Err(e.to_string())
        }
    };

    result
}

/// Process video through all steps: download, transcribe, translate, TTS, and merge
#[tauri::command]
pub async fn process_video(
    url: String,
    output_path: String,
    target_language: String,
    target_language_name: String,
    api_key: String,
    voice: String,
    model: String,
    words_per_second: f64,
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
    info!("  Voice: {}", voice);
    info!("  Model: {}", model);
    info!("  Words per second: {}", words_per_second);

    let output_dir = PathBuf::from(&output_path);

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

    // Updated Step 4: Generate TTS
    info!("Step 4: Generating speech");
    let tts_result = generate_speech(
        translation_result.translated_vtt_path.clone(),
        output_path.clone(),
        api_key.clone(),
        voice,
        model,
        words_per_second,
        translation_result.base_filename.clone(),
        target_language.clone(),
        window.clone(),
    )
    .await
    .map_err(|e| {
        error!("TTS generation failed: {}", e);
        format!("TTS generation failed: {}", e)
    })?;
    info!("TTS generation completed successfully");
    info!("  TTS audio path: {}", tts_result.audio_path);

    // Step 5: Final merge
    info!("Step 5: Merging final video");
    let merge_result = match merge_media(
        download_result.video_path.clone(),
        tts_result.audio_path.clone(),
        download_result.audio_path.clone(),
        transcription_result.vtt_path.clone(),
        translation_result.translated_vtt_path.clone(),
        output_path.clone(),
        "auto".to_string(),      // source language code - auto detect
        target_language.clone(), // target language code
        window.clone(),
    )
    .await
    {
        Ok(result) => {
            info!("Merge completed successfully");
            info!("  Final output path: {}", result.output_path);
            result
        }
        Err(e) => {
            error!("Merge failed: {}", e);
            return Err(format!("Merge failed: {}", e));
        }
    };

    info!("=== Video Processing Pipeline Completed Successfully ===");

    Ok(ProcessVideoResult {
        video_path: download_result.video_path,
        audio_path: download_result.audio_path,
        transcription_path: transcription_result.vtt_path,
        translation_path: translation_result.translated_vtt_path,
        tts_path: tts_result.audio_path,
        final_path: merge_result.output_path,
    })
}
