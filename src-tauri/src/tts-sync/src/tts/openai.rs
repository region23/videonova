//! Модуль для интеграции с OpenAI API
//! 
//! Этот модуль содержит функции для генерации речи с использованием OpenAI API
//! с поддержкой отслеживания прогресса.

use std::path::Path;
use std::sync::Arc;
use futures::future::join_all;
use reqwest::Client;
use tokio::sync::Semaphore;
use regex::Regex;
use crate::subtitle::parser::Subtitle;
use crate::config::TtsSyncConfig;
use crate::error::{Result, TtsSyncError};
use crate::progress::ProgressTracker;
use log;

/// Сегмент для TTS
#[derive(Debug, Clone)]
pub struct TtsSegment {
    /// Оригинальный субтитр
    pub original_subtitle: Subtitle,
    /// Текст для TTS
    pub text: String,
    /// Путь к аудиофайлу
    pub audio_file: Option<String>,
    /// Путь к обработанному аудиофайлу
    pub processed_audio_file: Option<String>,
    /// Параметры постобработки
    pub post_processing: Option<PostProcessingParams>,
}

/// Параметры постобработки аудио
#[derive(Debug, Clone)]
pub struct PostProcessingParams {
    /// Применять изменение темпа
    pub apply_tempo_adjustment: bool,
    /// Коэффициент изменения темпа
    pub tempo_factor: f64,
}

/// Генерация речи с использованием OpenAI API
pub async fn generate_speech(
    subtitles: &[Subtitle],
    config: &TtsSyncConfig,
) -> Result<String> {
    // Вызываем функцию с отслеживанием прогресса, но без трекера
    generate_speech_with_progress(subtitles, config, None).await
}

/// Генерация речи с отслеживанием прогресса
pub async fn generate_speech_with_progress(
    subtitles: &[Subtitle],
    config: &TtsSyncConfig,
    tracker: Option<&ProgressTracker>,
) -> Result<String> {
    // Подготавливаем сегменты для TTS
    let segments = prepare_segments_for_tts(subtitles);
    
    // Генерируем речь для каждого сегмента
    let output_path = generate_speech_for_segments(&segments, config, tracker).await?;
    
    Ok(output_path)
}

/// Подготовка сегментов для TTS
fn prepare_segments_for_tts(subtitles: &[Subtitle]) -> Vec<TtsSegment> {
    let mut segments = Vec::new();
    
    for subtitle in subtitles {
        let text = prepare_text_for_tts(&subtitle.text);
        
        segments.push(TtsSegment {
            original_subtitle: subtitle.clone(),
            text,
            audio_file: None,
            processed_audio_file: None,
            post_processing: None,
        });
    }
    
    segments
}

/// Подготовка текста для TTS
fn prepare_text_for_tts(text: &str) -> String {
    // Удаляем HTML-теги
    let html_regex = Regex::new(r"<[^>]*>").unwrap();
    let text = html_regex.replace_all(text, "");
    
    // Заменяем специальные символы
    let text = text.to_string()
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'");
    
    // Нормализуем пробелы
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

/// Генерация речи для сегментов
async fn generate_speech_for_segments(
    segments: &[TtsSegment],
    config: &TtsSyncConfig,
    tracker: Option<&ProgressTracker>,
) -> Result<String> {
    // Validate API key
    if config.openai_api_key.trim().is_empty() {
        log::error!("OpenAI API key is empty");
        return Err(TtsSyncError::Configuration("OpenAI API key is required for TTS generation".to_string()));
    }

    // Создаем клиент для запросов к API
    let client = Client::new();
    
    // Validate API key by making a test request
    log::info!("Validating OpenAI API key...");
    if let Some(t) = tracker {
        t.notify_progress(0.0, Some("Проверка API ключа OpenAI".to_string()));
    }

    log::debug!("Making test request to OpenAI API to validate key...");
    let test_response = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", config.openai_api_key))
        .send()
        .await;

    match test_response {
        Ok(response) if !response.status().is_success() => {
            let status = response.status();
            let error_text = match response.text().await {
                Ok(text) => text,
                Err(e) => format!("Failed to read error response: {}", e),
            };
            log::error!("OpenAI API key validation failed (status {}): {}", status, error_text);
            return Err(TtsSyncError::Configuration(format!("Invalid OpenAI API key: {} (status {})", error_text, status)));
        }
        Err(e) => {
            log::error!("Failed to validate OpenAI API key: {}", e);
            return Err(TtsSyncError::Configuration(format!("Failed to validate OpenAI API key: {}", e)));
        }
        Ok(_) => log::info!("OpenAI API key validated successfully"),
    }
    
    // Log configuration details
    log::info!("TTS Configuration:");
    log::info!("  Model: {}", config.tts_model.as_str());
    log::info!("  Voice: {}", config.tts_voice.as_str());
    log::info!("  Max concurrent requests: {}", config.max_concurrent_requests);
    
    // Создаем семафор для ограничения количества одновременных запросов
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_requests));
    
    // Создаем временную директорию для аудиофайлов
    let temp_dir = tempfile::tempdir()?;
    let temp_dir_path = temp_dir.path().to_path_buf();
    
    // Создаем задачи для генерации речи
    let mut tasks = Vec::new();
    let total_segments = segments.len();
    
    // Update progress to indicate starting TTS generation
    if let Some(t) = tracker {
        t.notify_progress(0.0, Some("Начало генерации речи".to_string()));
    }

    // Track completed segments for progress updates
    let completed_segments = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let progress_segments = completed_segments.clone();
    
    for (i, segment) in segments.iter().enumerate() {
        let client = client.clone();
        let api_key = config.openai_api_key.clone();
        let model = config.tts_model.as_str().to_string();
        let voice = config.tts_voice.as_str().to_string();
        let text = segment.text.clone();
        let semaphore = semaphore.clone();
        let temp_dir_path = temp_dir_path.clone();
        let completed_segments = completed_segments.clone();
        
        // Создаем задачу для генерации речи
        let task = tokio::spawn(async move {
            // Получаем разрешение от семафора
            let _permit = semaphore.acquire().await.unwrap();
            
            // Генерируем имя файла
            let file_name = format!("segment_{}.mp3", i);
            let file_path = temp_dir_path.join(file_name);
            
            // Отправляем запрос к API
            log::info!("Sending TTS request to OpenAI API for segment {}", i);
            let response = client
                .post("https://api.openai.com/v1/audio/speech")
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&serde_json::json!({
                    "model": model,
                    "voice": voice,
                    "input": text,
                    "response_format": "mp3",
                    "speed": 1.0
                }))
                .send()
                .await;
            
            match response {
                Ok(response) => {
                    if response.status().is_success() {
                        log::info!("Successfully generated TTS for segment {}", i);
                        // Получаем байты ответа
                        let bytes = match response.bytes().await {
                            Ok(bytes) if !bytes.is_empty() => bytes,
                            Ok(_) => {
                                log::error!("Received empty response for segment {}", i);
                                return (i, None);
                            }
                            Err(e) => {
                                log::error!("Failed to read response bytes for segment {}: {}", i, e);
                                return (i, None);
                            }
                        };
                        
                        // Записываем в файл
                        if let Err(e) = tokio::fs::write(&file_path, &bytes).await {
                            log::error!("Failed to save TTS audio for segment {} to {}: {}", i, file_path.display(), e);
                            return (i, None);
                        }
                        
                        log::info!("Saved TTS audio for segment {} to {}", i, file_path.display());
                        
                        // Update progress counter
                        completed_segments.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                        return (i, Some(file_path.to_string_lossy().to_string()));
                    } else {
                        let status = response.status();
                        let error_text = match response.text().await {
                            Ok(text) => text,
                            Err(e) => format!("Failed to read error response: {}", e),
                        };
                        log::error!("OpenAI API error for segment {} (status {}): {}", i, status, error_text);
                    }
                    
                    (i, None)
                },
                Err(e) => {
                    log::error!("Failed to send TTS request for segment {}: {}", i, e);
                    (i, None)
                },
            }
        });
        
        tasks.push(task);

        // Update progress after each task is created
        if let Some(t) = tracker {
            let completed = progress_segments.load(std::sync::atomic::Ordering::SeqCst);
            let progress = (completed as f32 / total_segments as f32) * 100.0;
            t.notify_progress(progress, Some(format!(
                "Генерация речи: {}/{} сегментов",
                completed,
                total_segments
            )));
        }
    }
    
    // Ожидаем завершения всех задач
    let results = join_all(tasks).await;
    
    // Обрабатываем результаты
    let mut segments = segments.to_vec();
    let mut failed_segments = Vec::new();

    for result in results {
        if let Ok((i, file_path)) = result {
            if let Some(path) = file_path {
                segments[i].audio_file = Some(path);
                segments[i].processed_audio_file = segments[i].audio_file.clone();
            } else {
                failed_segments.push(i);
            }
        }
    }

    // Check if any segments failed
    if !failed_segments.is_empty() {
        log::error!("Failed to generate TTS for segments: {:?}", failed_segments);
        return Err(TtsSyncError::TtsGeneration(format!(
            "Failed to generate TTS for {} segments",
            failed_segments.len()
        )));
    }
    
    // Объединяем аудиофайлы
    if let Some(t) = tracker {
        t.notify_progress(95.0, Some("Объединение аудиофайлов".to_string()));
    }

    let output_path = concat_audio_files(&segments, &temp_dir_path)?;

    if let Some(t) = tracker {
        t.notify_progress(100.0, Some("Генерация речи завершена".to_string()));
    }
    
    Ok(output_path)
}

/// Объединение аудиофайлов
fn concat_audio_files(segments: &[TtsSegment], temp_dir: &Path) -> Result<String> {
    // Создаем файл со списком аудиофайлов для FFmpeg
    let concat_list_path = temp_dir.join("concat_list.txt");
    let mut concat_list = std::fs::File::create(&concat_list_path)?;
    
    // Записываем список файлов
    for segment in segments {
        if let Some(audio_file) = &segment.processed_audio_file {
            use std::io::Write;
            writeln!(concat_list, "file '{}'", audio_file)?;
        }
    }
    
    // Закрываем файл
    drop(concat_list);
    
    // Создаем выходной файл
    let output_path = temp_dir.join("output.mp3");
    
    // Запускаем FFmpeg для объединения файлов
    let status = std::process::Command::new("ffmpeg")
        .args(&[
            "-f", "concat",
            "-safe", "0",
            "-i", concat_list_path.to_str().unwrap(),
            "-c", "copy",
            output_path.to_str().unwrap(),
        ])
        .status()?;
    
    if !status.success() {
        return Err(TtsSyncError::AudioProcessing("Failed to concatenate audio files".to_string()));
    }
    
    Ok(output_path.to_string_lossy().to_string())
}
