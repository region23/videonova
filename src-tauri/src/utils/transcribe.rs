use anyhow::{anyhow, Result};
use log::{info, error, debug, warn};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use crate::utils::common::{sanitize_filename, check_file_exists_and_valid};
use serde_json::{self, Value};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranscriptionProgress {
    pub status: String,
    pub progress: f32,
}

// Добавляем атрибут #[allow(dead_code)] к неиспользуемым вариантам enum
#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ResponseFormat {
    Json,
    Text,
    Srt,
    VerboseJson,
    Vtt,
}

impl Default for ResponseFormat {
    fn default() -> Self {
        ResponseFormat::Vtt
    }
}

impl ToString for ResponseFormat {
    fn to_string(&self) -> String {
        match self {
            ResponseFormat::Json => "json".to_string(),
            ResponseFormat::Text => "text".to_string(),
            ResponseFormat::Srt => "srt".to_string(),
            ResponseFormat::VerboseJson => "verbose_json".to_string(),
            ResponseFormat::Vtt => "vtt".to_string(),
        }
    }
}

// Enum для указания гранулярности временных меток
#[derive(Debug, Clone, PartialEq)]
pub enum TimestampGranularity {
    Segment,
    Word,
}

impl ToString for TimestampGranularity {
    fn to_string(&self) -> String {
        match self {
            TimestampGranularity::Segment => "segment".to_string(),
            TimestampGranularity::Word => "word".to_string(),
        }
    }
}

impl Default for TimestampGranularity {
    fn default() -> Self {
        TimestampGranularity::Segment
    }
}

#[derive(Debug)]
struct MultipartFormBuilder {
    boundary: String,
    body: Vec<u8>,
}

impl MultipartFormBuilder {
    const DEFAULT_BOUNDARY: &'static str = "--------------------boundary";

    fn new() -> Self {
        Self {
            boundary: Self::DEFAULT_BOUNDARY.to_string(),
            body: Vec::new(),
        }
    }

    // Добавляем атрибут #[allow(dead_code)] к неиспользуемой функции
    #[allow(dead_code)]
    fn with_boundary(boundary: &str) -> Self {
        Self {
            boundary: boundary.to_string(),
            body: Vec::new(),
        }
    }

    fn add_text(&mut self, name: &str, value: &str) -> &mut Self {
        self.body.extend_from_slice(format!("--{}\r\n", self.boundary).as_bytes());
        self.body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes());
        self.body.extend_from_slice(value.as_bytes());
        self.body.extend_from_slice(b"\r\n");
        self
    }

    fn add_file(&mut self, name: &str, filename: &str, content: &[u8], content_type: &str) -> &mut Self {
        self.body.extend_from_slice(format!("--{}\r\n", self.boundary).as_bytes());
        self.body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                name, filename
            )
            .as_bytes(),
        );
        self.body.extend_from_slice(format!("Content-Type: {}\r\n\r\n", content_type).as_bytes());
        self.body.extend_from_slice(content);
        self.body.extend_from_slice(b"\r\n");
        self
    }

    fn finish(&mut self) -> Vec<u8> {
        self.body.extend_from_slice(format!("--{}--\r\n", self.boundary).as_bytes());
        std::mem::take(&mut self.body)
    }

    fn content_type(&self) -> String {
        format!("multipart/form-data; boundary={}", self.boundary)
    }
}

// Функция для преобразования verbose_json с word-level timestamps в VTT формат
async fn convert_verbose_json_to_vtt(json_content: &str) -> Result<String> {
    // Парсим JSON
    let json: Value = serde_json::from_str(json_content)?;
    
    // Проверяем, что это действительно JSON с word-level timestamps
    if !json.is_object() || !json.as_object().unwrap().contains_key("words") {
        return Err(anyhow!("Invalid JSON format: expected an object with 'words' field"));
    }
    
    // Начинаем формировать VTT
    let mut vtt_content = String::from("WEBVTT\n\n");
    
    // Получаем все слова
    let words = json["words"].as_array().ok_or_else(|| anyhow!("Expected 'words' to be an array"))?;
    
    if words.is_empty() {
        return Ok(vtt_content); // Пустой результат, но валидный VTT
    }
    
    // Настройки для подстройки синхронизации
    let min_pause_for_new_segment = 0.8; // Секунд паузы для нового сегмента (было 1.0)
    let long_pause_threshold = 2.0; // Порог для определения длительной паузы (смена сцены)
    let start_offset_normal = 0.2; // Обычная задержка перед показом субтитров
    let start_offset_after_long_pause = 0.0; // Нет задержки после длительной паузы
    let min_segment_duration = 1.0; // Минимальная длительность сегмента в секундах
    
    // Группируем слова в предложения/сегменты
    let mut segments = Vec::new();
    let mut current_segment = Vec::new();
    let mut last_end_time = 0.0;
    let mut is_after_long_pause = Vec::new(); // Отмечаем, следует ли сегмент после длинной паузы
    
    for word in words {
        let start = word["start"].as_f64().ok_or_else(|| anyhow!("Word missing 'start' timestamp"))?;
        let end = word["end"].as_f64().ok_or_else(|| anyhow!("Word missing 'end' timestamp"))?;
        let text = word["word"].as_str().ok_or_else(|| anyhow!("Word missing 'word' text"))?;
        
        let pause_duration = if current_segment.is_empty() && last_end_time > 0.0 {
            // Вычисляем паузу только между сегментами
            start - last_end_time
        } else {
            0.0 // Внутри сегмента не считаем паузы
        };
        
        // Определяем, есть ли здесь новый сегмент
        if current_segment.is_empty() || start - last_end_time > min_pause_for_new_segment {
            if !current_segment.is_empty() {
                segments.push(current_segment);
                is_after_long_pause.push(pause_duration >= long_pause_threshold);
                current_segment = Vec::new();
            }
        }
        
        current_segment.push((start, end, text));
        last_end_time = end;
    }
    
    // Добавляем последний сегмент
    if !current_segment.is_empty() {
        segments.push(current_segment);
        is_after_long_pause.push(false); // Последний сегмент не может быть после длинной паузы
    }
    
    // Форматируем каждый сегмент в VTT формат
    for (i, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            continue;
        }

        // Определяем смещение времени в зависимости от того, следует ли сегмент после длинной паузы
        let follows_long_pause = is_after_long_pause.get(i).copied().unwrap_or(false);
        let offset = if follows_long_pause {
            // Для сегментов после длинной паузы (смена сцены/диалога) не добавляем задержку
            info!("Segment {} follows a long pause, using exact timing", i);
            start_offset_after_long_pause
        } else {
            // Для обычных сегментов добавляем стандартную задержку
            start_offset_normal
        };
        
        // Применяем смещение к времени начала для лучшей синхронизации
        let start_time = segment.first().unwrap().0 + offset;
        let end_time = segment.last().unwrap().1;
        
        // Убеждаемся, что сегмент не слишком короткий
        let duration = end_time - start_time;
        let adjusted_end_time = if duration < min_segment_duration {
            start_time + min_segment_duration
        } else {
            end_time
        };
        
        // Форматируем времена в формат VTT (HH:MM:SS.mmm)
        let start_formatted = format_timestamp(start_time);
        let end_formatted = format_timestamp(adjusted_end_time);
        
        // Объединяем все слова в сегменте
        let text: String = segment.iter()
            .map(|(_, _, word)| word.to_string())
            .collect::<Vec<String>>()
            .join(" ")
            .trim()
            .to_string();
        
        // Добавляем сегмент в VTT без порядкового номера
        vtt_content.push_str(&format!("{} --> {}\n{}\n\n", start_formatted, end_formatted, text));
    }
    
    Ok(vtt_content)
}

// Вспомогательная функция для форматирования временной метки в формат VTT
fn format_timestamp(seconds: f64) -> String {
    let hours = (seconds / 3600.0).floor() as u32;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
    let seconds_remainder = seconds % 60.0;
    
    format!("{:02}:{:02}:{:06.3}", hours, minutes, seconds_remainder)
}

// Функция для выполнения отдельного запроса на получение JSON с таймстампами
async fn fetch_json_with_timestamps(
    audio_path: &Path,
    api_key: &str,
    language: Option<String>,
) -> Result<String> {
    info!("Fetching JSON with word-level timestamps");
    
    // Читаем файл целиком в память
    let file_content = tokio::fs::read(audio_path).await
        .map_err(|e| anyhow!("Failed to read audio file: {}", e))?;

    // Создаем multipart form-data
    let mut form = MultipartFormBuilder::new();
    let filename = audio_path.file_name().unwrap().to_string_lossy();
    
    // Формируем запрос в точности как в примере curl
    // 1. Сначала добавляем файл
    form.add_file("file", &filename, &file_content, "application/octet-stream");
    
    // 2. Затем timestamp_granularities[] - ОБЯЗАТЕЛЬНО С []
    form.add_text("timestamp_granularities[]", "word");
    
    // 3. Модель
    form.add_text("model", "whisper-1");
    
    // 4. Формат ответа
    form.add_text("response_format", "verbose_json");
    
    // 5. Язык, если указан
    if let Some(lang) = &language {
        form.add_text("language", lang);
    }
    
    // Получаем тело запроса
    let body = form.finish();
    
    // Логируем детали запроса
    info!("Sending request with exact curl-like parameter order");
    info!("Content-Type: {}", form.content_type());
    let preview_size = body.len().min(500);
    info!("Request body preview (first {} bytes): {:?}", preview_size, String::from_utf8_lossy(&body[0..preview_size]));
    
    // Создаем HTTP клиент
    let client = reqwest::Client::new();
    
    // Отправляем запрос
    info!("Sending dedicated request for JSON with word timestamps");
    let response = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", form.content_type())
        .body(body)
        .send()
        .await
        .map_err(|e| anyhow!("Failed to connect to OpenAI API: {}", e))?;
    
    // Проверяем статус ответа
    if !response.status().is_success() {
        let error_text = response.text().await?;
        return Err(anyhow!("API request failed: {}", error_text));
    }
    
    // Получаем текст ответа
    let content = response.text().await?;
    
    // Проверяем, что ответ действительно JSON
    if !content.starts_with('{') {
        return Err(anyhow!("Expected JSON response, but got: {}", content.chars().take(50).collect::<String>()));
    }
    
    info!("Successfully received JSON with word timestamps");
    Ok(content)
}

pub async fn transcribe_audio(
    audio_path: &Path,
    output_dir: &Path,
    api_key: &str,
    language: Option<String>,
    progress_sender: Option<mpsc::Sender<TranscriptionProgress>>,
    timestamp_granularity: Option<TimestampGranularity>,
) -> Result<PathBuf> {
    info!("Starting transcription process");
    
    // Add debug info about timestamp granularity
    if let Some(gran) = &timestamp_granularity {
        info!("Using timestamp granularity: {:?}", gran);
    } else {
        info!("Using default segment-level timestamps");
    }
    
    // Validate API key
    if api_key.trim().is_empty() {
        error!("OpenAI API key is empty");
        return Err(anyhow!("OpenAI API key is required for transcription"));
    }
    
    // Определяем, какой формат ответа использовать на основе гранулярности
    let (format, granularity) = match timestamp_granularity {
        Some(TimestampGranularity::Word) => (ResponseFormat::VerboseJson, Some(TimestampGranularity::Word)),
        _ => (ResponseFormat::Vtt, None),
    };
    
    // Добавляем логирование для отладки
    info!("Request configuration:");
    info!("  - Response format: {:?}", format);
    if let Some(gran) = &granularity {
        info!("  - Timestamp granularity: {:?}", gran);
    }
    
    // Расширение выходного файла зависит от формата ответа, но результат всегда будет VTT
    let file_extension = "vtt";
    
    // Create output directory if it doesn't exist
    if let Err(e) = fs::create_dir_all(output_dir).await {
        error!("Failed to create output directory: {}", e);
        return Err(anyhow!("Failed to create output directory: {}", e));
    }
    
    // Проверяем существование файла
    if !audio_path.exists() {
        error!("Audio file does not exist");
        return Err(anyhow!("Audio file does not exist"));
    }
    
    // Проверяем права доступа к файлу
    let metadata = match std::fs::metadata(audio_path) {
        Ok(meta) => meta,
        Err(e) => {
            error!("Failed to get file metadata: {}", e);
            return Err(anyhow!("Failed to get file metadata: {}", e));
        }
    };
    
    if !metadata.is_file() {
        error!("Path is not a file");
        return Err(anyhow!("Path is not a file"));
    }
    
    // Define output file path (same name as input but with appropriate extension)
    let file_stem = audio_path
        .file_stem()
        .ok_or_else(|| anyhow!("Failed to get file stem"))?
        .to_string_lossy();
    
    // Обрабатываем имя файла - переводим в нижний регистр и заменяем пробелы на подчеркивания
    let sanitized_file_stem = sanitize_filename(&file_stem);
    let output_path = output_dir.join(format!("{}.{}", sanitized_file_stem, file_extension));
    
    // Define a path for the raw API response (for debugging)
    let raw_response_path = output_dir.join(format!("{}_raw_response.txt", sanitized_file_stem));

    // Check if transcription file already exists
    if check_file_exists_and_valid(&output_path).await {
        info!("Found existing transcription file");
        return Ok(output_path);
    }

    // Send initial progress
    if let Some(sender) = &progress_sender {
        sender
            .send(TranscriptionProgress {
                status: "Preparing transcription".to_string(),
                progress: 0.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }

    // Читаем файл целиком в память
    let file_content = match tokio::fs::read(audio_path).await {
        Ok(content) => content,
        Err(e) => {
            error!("Failed to read audio file: {}", e);
            return Err(anyhow!("Failed to read audio file: {}", e));
        }
    };

    // Создаем multipart form-data с помощью builder'а
    let mut form = MultipartFormBuilder::new();
    let filename = audio_path.file_name().unwrap().to_string_lossy();
    
    // Логируем информацию о файле
    info!("Processing audio file: {}", filename);
    
    // Формируем запрос в зависимости от требуемого формата
    if format == ResponseFormat::VerboseJson {
        info!("Setting curl-style parameter order for word-level timestamps");
        
        // 1. Сначала добавляем файл
        form.add_file("file", &filename, &file_content, "application/octet-stream");
        
        // 2. Добавляем timestamp_granularities[] - с квадратными скобками
        if let Some(gran) = &granularity {
            info!("Adding timestamp_granularities[] = {}", gran.to_string());
            form.add_text("timestamp_granularities[]", &gran.to_string());
        }
        
        // 3. Добавляем модель
        form.add_text("model", "whisper-1");
        
        // 4. Указываем формат ответа verbose_json
        form.add_text("response_format", "verbose_json");
        
        // 5. Добавляем язык если есть
        if let Some(lang) = &language {
            info!("Using specified language: {}", lang);
            form.add_text("language", lang);
        } else {
            info!("Using auto language detection");
        }
    } else {
        // Стандартный порядок для обычного VTT
        // 1. Сначала файл
        form.add_file("file", &filename, &file_content, "application/octet-stream");
        
        // 2. Затем модель
        form.add_text("model", "whisper-1");
        
        // 3. Формат ответа
        form.add_text("response_format", &format.to_string());
            
        // 4. Язык если указан
        if let Some(lang) = &language {
            info!("Using specified language: {}", lang);
            form.add_text("language", lang);
        } else {
            info!("Using auto language detection");
        }
    }

    // Получаем финальное тело запроса
    let body = form.finish();
    
    // Логируем детали multipart form
    info!("Multipart form boundary: {}", MultipartFormBuilder::DEFAULT_BOUNDARY);
    info!("Content-Type: {}", form.content_type());
    
    // Выводим первые 500 байт запроса для отладки (без бинарного содержимого файла)
    let preview_size = body.len().min(500);
    info!("Request body preview (first {} bytes): {:?}", preview_size, String::from_utf8_lossy(&body[0..preview_size]));
    
    // Send progress update - preparing the request
    if let Some(sender) = &progress_sender {
        sender
            .send(TranscriptionProgress {
                status: "Preparing request to OpenAI".to_string(),
                progress: 5.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }
    
    // Create the client and request
    let client = reqwest::Client::new();
    
    // Отправляем запрос
    info!("Sending request to OpenAI Whisper API");
    
    let response_result = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", form.content_type())
        .body(body)
        .send()
        .await;
    
    match response_result {
        Ok(response) => {
            let status = response.status();
            info!("OpenAI API response status: {}", status);
            
            // Получаем все заголовки ответа
            info!("All response headers:");
            for (name, value) in response.headers().iter() {
                info!("  {}: {}", name, value.to_str().unwrap_or("Non-UTF8 value"));
            }
            
            // Send progress update
            if let Some(sender) = &progress_sender {
                sender
                    .send(TranscriptionProgress {
                        status: format!("Processing transcription result (HTTP {})", status.as_u16()),
                        progress: 90.0,
                    })
                    .await
                    .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
            }
            
            // Check if request was successful
            if !status.is_success() {
                let error_text = response.text().await?;
                error!("OpenAI API error: HTTP {}", status);
                return Err(anyhow!("API request failed (HTTP {}): {}", status, error_text));
            }
            
            // Get response text
            let content = response.text().await?;
            
            // Сохраняем оригинальный ответ API для анализа
            info!("Saving raw API response to: {}", raw_response_path.display());
            fs::write(&raw_response_path, &content).await
                .map_err(|e| anyhow!("Failed to save raw API response: {}", e))?;
            
            // Проверяем формат ответа для дебага
            let first_line = content.lines().next().unwrap_or("").trim();
            let content_preview: String = content.chars().take(200).collect();
            info!("Response first line: '{}'", first_line);
            info!("Response preview: '{}'", content_preview.replace("\n", "\\n"));
            
            // Опредляем формат ответа на основе его содержимого
            let detected_format = if first_line == "WEBVTT" {
                info!("Detected VTT format in response (despite request format: {:?})", format);
                "vtt"
            } else if content.starts_with('{') && content.contains("\"text\":") {
                info!("Detected JSON format in response");
                "json"
            } else {
                info!("Unknown response format");
                "unknown"
            };
            
            // Send progress update
            if let Some(sender) = &progress_sender {
                sender
                    .send(TranscriptionProgress {
                        status: "Processing transcription".to_string(),
                        progress: 95.0,
                    })
                    .await
                    .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
            }
            
            // Если использовался word-level granularity, конвертируем JSON в VTT
            let final_content = if format == ResponseFormat::VerboseJson && detected_format == "json" {
                info!("Converting verbose JSON with word timestamps to VTT format");
                convert_verbose_json_to_vtt(&content).await?
            } else {
                if format == ResponseFormat::VerboseJson && detected_format != "json" {
                    warn!("Expected JSON response for word-level timestamps but received different format");
                    warn!("This is likely because OpenAI is ignoring our request for verbose_json with timestamp_granularities");
                    warn!("Trying to make a separate request for JSON with word timestamps...");
                    
                    // Делаем отдельный запрос для получения JSON с таймстампами
                    match fetch_json_with_timestamps(audio_path, api_key, language.clone()).await {
                        Ok(json_content) => {
                            // Сохраняем JSON для анализа
                            let json_path = output_dir.join(format!("{}_timestamps.json", sanitized_file_stem));
                            info!("Saving word timestamps JSON to: {}", json_path.display());
                            fs::write(&json_path, &json_content).await
                                .map_err(|e| anyhow!("Failed to save JSON response: {}", e))?;
                            
                            // Конвертируем JSON в VTT
                            info!("Converting JSON with word timestamps to VTT");
                            match convert_verbose_json_to_vtt(&json_content).await {
                                Ok(converted_vtt) => {
                                    info!("Successfully converted JSON to VTT with word-level timestamps");
                                    converted_vtt
                                },
                                Err(e) => {
                                    warn!("Failed to convert JSON to VTT: {}", e);
                                    info!("Using original VTT response instead");
                                    content
                                }
                            }
                        },
                        Err(e) => {
                            warn!("Failed to fetch JSON with word timestamps: {}", e);
                            warn!("Using original VTT response instead");
                            content
                        }
                    }
                } else {
                    content
                }
            };
            
            // Write content to file
            let mut output_file = File::create(&output_path).await?;
            output_file.write_all(final_content.as_bytes()).await?;
            
            // Send completion progress
            if let Some(sender) = &progress_sender {
                sender
                    .send(TranscriptionProgress {
                        status: "Transcription complete".to_string(),
                        progress: 100.0,
                    })
                    .await
                    .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
            }
            
            info!("Transcription completed successfully");
            info!("VTT output saved to: {}", output_path.display());
            info!("Raw API response saved to: {}", raw_response_path.display());
            
            Ok(output_path)
        },
        Err(err) => {
            error!("Failed to connect to OpenAI API: {}", err);
            Err(anyhow!("Failed to connect to OpenAI API: {}", err))
        }
    }
} 