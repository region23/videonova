use reqwest;
use serde::Serialize;
use std::path::PathBuf;
use tauri::Emitter;
use tokio::sync::mpsc;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use anyhow::anyhow;

use crate::utils::youtube::{self, DownloadProgress, VideoInfo};
use crate::utils::transcribe;
use crate::utils::translate;
use crate::utils::tts;
use crate::utils::merge;
use crate::utils::common::sanitize_filename;

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
pub struct MergeResult {
    output_path: String,
    output_dir: String,
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
    // Create progress channel
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
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            // Emit progress event to frontend
            if let Err(e) = progress_window.emit("download-progress", progress.clone()) {
                eprintln!("Failed to emit progress: {}", e);
            }
            
            // Check if audio download is complete
            if progress.component == "audio" && progress.progress >= 99.0 && 
               !audio_completed_clone.load(Ordering::Relaxed) {
                
                // Mark as completed to avoid duplicate events
                audio_completed_clone.store(true, Ordering::Relaxed);
                
                // Используем отдельную задачу для обработки события audio-ready,
                // чтобы не блокировать мониторинг загрузки видео
                let event_window = progress_window.clone();
                let url_event = url_clone.clone();
                let output_path_event = output_path_clone.clone();
                
                tokio::spawn(async move {
                    // Добавляем небольшую задержку для стабильности
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    
                    // Для события audio-ready нам нужен только путь к аудиофайлу
                    if let Ok(info) = youtube::get_video_info(&url_event).await {
                        let output_dir = PathBuf::from(&output_path_event);
                        let safe_title = sanitize_filename(&info.title);
                        let audio_path = output_dir.join(format!("{}_audio.m4a", safe_title));
                        
                        // Emit audio ready event with path
                        if let Err(e) = event_window.emit("audio-ready", audio_path.to_string_lossy().to_string()) {
                            eprintln!("Failed to emit audio-ready event: {}", e);
                        }
                    }
                });
            }
        }
    });

    // Start download
    let output_dir = PathBuf::from(output_path);
    let result = youtube::download_video(&url, &output_dir, Some(tx))
        .await
        .map_err(|e| e.to_string())?;

    // Создаем результат для возврата
    let download_result = DownloadResult {
        video_path: result.video_path.to_string_lossy().to_string(),
        audio_path: result.audio_path.to_string_lossy().to_string(),
    };
    
    // Отправляем событие download-complete с результатом
    if let Err(e) = window.emit("download-complete", download_result.clone()) {
        eprintln!("Failed to emit download-complete event: {}", e);
    }

    Ok(download_result)
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
    
    let result_path = transcribe::transcribe_audio(
        &audio_file,
        &output_dir,
        &api_key,
        language,
        Some(tx),
    )
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
    let filename = vtt_file.file_stem()
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
    original_vtt_path: String,
    translated_vtt_path: String,
    output_path: String,
    window: tauri::Window,
) -> Result<MergeResult, String> {
    // Create progress channel
    let (tx, mut rx) = mpsc::channel::<merge::MergeProgress>(32);

    // Clone window handle for the progress monitoring task
    let progress_window = window.clone();

    // Spawn progress monitoring task
    let monitoring_task = tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            // Emit progress event to frontend
            if let Err(e) = progress_window.emit("merge-progress", progress) {
                eprintln!("Failed to emit merge progress: {}", e);
            }
        }
    });

    // Start merge process
    let video_file = PathBuf::from(&video_path);
    let translated_audio_file = PathBuf::from(&translated_audio_path);
    let original_vtt_file = PathBuf::from(&original_vtt_path);
    let translated_vtt_file = PathBuf::from(&translated_vtt_path);
    let output_dir = PathBuf::from(&output_path);
    
    let result_path = merge::merge_files(
        &video_file,
        &translated_audio_file,
        &original_vtt_file,
        &translated_vtt_file,
        &output_dir,
        Some(tx),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Wait for monitoring task to complete
    let _ = monitoring_task.await;

    // Создаем результат
    let result = MergeResult {
        output_path: result_path.to_string_lossy().to_string(),
        output_dir: output_dir.to_string_lossy().to_string(),
    };

    // Явно эмитим событие merge-complete
    if let Err(e) = window.emit("merge-complete", &result) {
        eprintln!("Failed to emit merge-complete event: {}", e);
    }

    Ok(result)
}
