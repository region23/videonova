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
    // Создаем клиент для запросов к API
    let client = Client::new();
    
    // Создаем семафор для ограничения количества одновременных запросов
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_requests));
    
    // Создаем временную директорию для аудиофайлов
    let temp_dir = tempfile::tempdir()?;
    let temp_dir_path = temp_dir.path().to_path_buf();
    
    // Создаем задачи для генерации речи
    let mut tasks = Vec::new();
    let total_segments = segments.len();
    
    for (i, segment) in segments.iter().enumerate() {
        let client = client.clone();
        let api_key = config.openai_api_key.clone();
        let model = config.tts_model.as_str().to_string();
        let voice = config.tts_voice.as_str().to_string();
        let text = segment.text.clone();
        let semaphore = semaphore.clone();
        let temp_dir_path = temp_dir_path.clone();
        
        // Создаем задачу для генерации речи
        let task = tokio::spawn(async move {
            // Получаем разрешение от семафора
            let _permit = semaphore.acquire().await.unwrap();
            
            // Генерируем имя файла
            let file_name = format!("segment_{}.mp3", i);
            let file_path = temp_dir_path.join(file_name);
            
            // Отправляем запрос к API
            let response = client
                .post("https://api.openai.com/v1/audio/speech")
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&serde_json::json!({
                    "model": model,
                    "voice": voice,
                    "input": text,
                }))
                .send()
                .await;
            
            match response {
                Ok(response) => {
                    if response.status().is_success() {
                        // Получаем байты ответа
                        let bytes = response.bytes().await.unwrap_or_default();
                        
                        // Записываем в файл
                        if let Ok(()) = tokio::fs::write(&file_path, &bytes).await {
                            return (i, Some(file_path.to_string_lossy().to_string()));
                        }
                    }
                    
                    (i, None)
                },
                Err(_) => (i, None),
            }
        });
        
        tasks.push(task);
        
        // Обновляем прогресс
        if let Some(t) = tracker {
            let progress = (i as f32 + 1.0) / total_segments as f32 * 100.0;
            t.notify_progress(progress, Some(format!("Генерация речи: {}/{} сегментов", i + 1, total_segments)));
        }
    }
    
    // Ожидаем завершения всех задач
    let results = join_all(tasks).await;
    
    // Обрабатываем результаты
    let mut segments = segments.to_vec();
    for result in results {
        if let Ok((i, file_path)) = result {
            if let Some(path) = file_path {
                segments[i].audio_file = Some(path);
                segments[i].processed_audio_file = segments[i].audio_file.clone();
            }
        }
    }
    
    // Объединяем аудиофайлы
    let output_path = concat_audio_files(&segments, &temp_dir_path)?;
    
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
