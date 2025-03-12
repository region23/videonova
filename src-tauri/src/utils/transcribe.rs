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
    let json: Value = serde_json::from_str(json_content)?;
    
    if !json.is_object() || !json.as_object().unwrap().contains_key("words") {
        return Err(anyhow!("Invalid JSON format: expected an object with 'words' field"));
    }
    
    // Получаем общую длительность аудио
    let total_duration = json["duration"].as_f64()
        .ok_or_else(|| anyhow!("Missing total duration in JSON"))?;
    
    let mut vtt_content = String::from("WEBVTT\n\n");
    let words = json["words"].as_array()
        .ok_or_else(|| anyhow!("Expected 'words' to be an array"))?;
    
    if words.is_empty() {
        return Ok(vtt_content);
    }

    #[derive(Debug)]
    struct TimingConfig {
        min_pause: f64,          // Минимальная пауза для нового сегмента
        long_pause: f64,         // Длинная пауза (новая сцена/контекст)
        max_segment_words: usize,// Максимальное количество слов в сегменте
        min_segment_words: usize,// Минимальное количество слов в сегменте
        min_duration: f64,       // Минимальная длительность показа
        max_duration: f64,       // Максимальная длительность показа
        words_per_minute: f64,   // Средняя скорость чтения
        overlap_threshold: f64,  // Максимальное перекрытие между сегментами
        reading_buffer: f64,     // Буфер для комфортного чтения (множитель)
        min_word_duration: f64,  // Минимальная длительность на одно слово
    }

    let config = TimingConfig {
        min_pause: 0.7,         // Увеличили для лучшего разделения фраз
        long_pause: 1.5,        // Вернули исходное значение
        max_segment_words: 14,  // Вернули исходное значение
        min_segment_words: 4,   // Увеличили минимальное количество слов в сегменте
        min_duration: 1.5,      // Увеличили минимальную длительность
        max_duration: 7.0,      // Вернули исходное значение
        words_per_minute: 150.0, // Уменьшили для более комфортной скорости чтения
        overlap_threshold: 0.3,  // Вернули исходное значение
        reading_buffer: 1.25,    // Увеличили буфер для более комфортного чтения
        min_word_duration: 0.25, // Увеличили минимальную длительность слова
    };

    #[derive(Debug)]
    struct Segment {
        start_time: f64,
        end_time: f64,
        words: Vec<String>,
        is_sentence_end: bool,
        natural_end: f64,  // Фактическое время окончания последнего слова
        min_gap: f64,      // Минимальный интервал до следующего сегмента
    }

    impl Segment {
        fn new(start: f64, end: f64, words: Vec<String>, is_end: bool, natural_end: f64) -> Self {
            Self {
                start_time: start,
                end_time: end,
                words,
                is_sentence_end: is_end,
                natural_end,
                min_gap: 0.05, // Минимальный интервал 50мс
            }
        }
    }

    fn is_sentence_end(text: &str) -> bool {
        let trimmed = text.trim_end();
        trimmed.ends_with('.') || 
        trimmed.ends_with('?') || 
        trimmed.ends_with('!') ||
        trimmed.ends_with("...") ||
        trimmed.ends_with('。') || // Japanese period
        trimmed.ends_with('？') || // Japanese question mark
        trimmed.ends_with('！')    // Japanese exclamation mark
    }

    // Добавляем функцию для определения естественных пауз в речи
    fn is_natural_pause(current_text: &str, next_text: Option<&str>) -> bool {
        let current = current_text.trim_end();
        
        // Расширенный список знаков препинания для пауз
        let pause_markers = [
            ',', ';', ':', '-', '–', '—',  // Тире разной длины
            '(', ')', '[', ']', '{', '}',  // Скобки
            '"', '"', '"', '«', '»'        // Кавычки разных типов
        ];
        
        // Проверяем знаки препинания
        if pause_markers.iter().any(|&mark| current.ends_with(mark)) {
            return true;
        }
        
        // Проверяем начало нового предложения
        if let Some(next) = next_text {
            let next_trimmed = next.trim_start();
            if next_trimmed.chars().next().map_or(false, |c| c.is_uppercase()) {
                // Для коротких фраз не считаем заглавную букву признаком паузы
                // чтобы избежать излишней фрагментации
                let words_count = current.split_whitespace().count();
                if words_count < 4 {
                    return false;
                }
                
                if current.ends_with('.') || 
                   current.ends_with('!') || 
                   current.ends_with('?') ||
                   current.ends_with('。') {
                    return true;
                }
            }
        }
        
        false
    }

    // Добавляем функцию для определения оптимальной длительности чтения
    fn calculate_reading_time(word_count: usize, text: &str, config: &TimingConfig) -> f64 {
        let base_reading_time = (word_count as f64 / config.words_per_minute) * 60.0;
        
        // Корректируем время чтения в зависимости от длины слов
        let avg_word_length = text.len() as f64 / word_count as f64;
        let length_factor = if avg_word_length > 8.0 {
            1.2 // Длинные слова читаются дольше
        } else if avg_word_length < 4.0 {
            0.9 // Короткие слова читаются быстрее
        } else {
            1.0
        };

        base_reading_time * length_factor
    }

    // Модифицированная функция расчета длительности показа
    fn calculate_display_duration(
        word_count: usize,
        text_duration: f64,
        next_segment_start: Option<f64>,
        config: &TimingConfig,
        total_duration: f64,
        current_start: f64,
        text: &str,
    ) -> f64 {
        // Базовое время чтения на основе количества слов
        let reading_time = calculate_reading_time(word_count, text, config);
        
        // Используем фактическую длительность произнесения как минимальную базу
        let base_duration = text_duration.max(reading_time);
        
        // Определяем максимально возможную длительность
        let max_possible = if let Some(next_start) = next_segment_start {
            // Если есть следующий сегмент, ограничиваем его началом
            let available = next_start - current_start;
            if available < text_duration {
                // Если доступного времени меньше чем длительность текста,
                // используем фактическую длительность
                text_duration
            } else {
                // Иначе можем использовать доступное время
                available.min(config.max_duration)
            }
        } else {
            // Для последнего сегмента используем оставшееся время
            (total_duration - current_start).min(config.max_duration)
        };

        // Добавляем буфер для комфортного чтения, но не превышаем max_possible
        let with_buffer = (base_duration * config.reading_buffer)
            .clamp(config.min_duration, max_possible);
        
        // Округляем до 3 знаков после запятой для избежания проблем с плавающей точкой
        (with_buffer * 1000.0).round() / 1000.0
    }

    let mut segments = Vec::new();
    let mut current_segment = Vec::new();
    let mut word_count = 0;
    let mut total_segments = 0;

    info!("Starting VTT conversion with {} words", words.len());

    for (i, word) in words.iter().enumerate() {
        let start = word["start"].as_f64()
            .ok_or_else(|| anyhow!("Word missing 'start' timestamp"))?;
        let end = word["end"].as_f64()
            .ok_or_else(|| anyhow!("Word missing 'end' timestamp"))?;
        let text = word["word"].as_str()
            .ok_or_else(|| anyhow!("Word missing 'word' text"))?;

        // Пропускаем пустые сегменты или сегменты только с пробелами
        if text.trim().is_empty() {
            continue;
        }

        let next_word = if i < words.len() - 1 {
            words[i + 1]["word"].as_str()
        } else {
            None
        };

        let pause_after = if i < words.len() - 1 {
            words[i + 1]["start"].as_f64().unwrap_or(end) - end
        } else {
            config.long_pause
        };

        current_segment.push((start, end, text.to_string()));
        word_count += 1;

        let should_split = 
            pause_after >= config.min_pause || 
            word_count >= config.max_segment_words || 
            is_sentence_end(text) || 
            is_natural_pause(text, next_word) ||
            i == words.len() - 1;

        // Добавляем сегмент только если в нем есть слова
        if should_split && !current_segment.is_empty() {
            let segment_start = current_segment.first().unwrap().0;
            let segment_end = current_segment.last().unwrap().1;
            let text: Vec<String> = current_segment.iter()
                .map(|(_,_,word)| word.clone())
                .collect();

            // Проверяем, что в сегменте есть непустой текст
            let joined_text = text.join(" ");
            let segment_text = joined_text.trim();
            if !segment_text.is_empty() {
                let next_segment_start = if i < words.len() - 1 {
                    words[i + 1]["start"].as_f64()
                } else {
                    None
                };

                let text_duration = segment_end - segment_start;
                let display_duration = calculate_display_duration(
                    word_count,
                    text_duration,
                    next_segment_start,
                    &config,
                    total_duration,
                    segment_start,
                    segment_text,
                );

                debug!(
                    "Segment {}: words={}, duration={:.2}s, display={:.2}s, text='{}'",
                    total_segments + 1,
                    word_count,
                    text_duration,
                    display_duration,
                    segment_text
                );

                segments.push(Segment::new(
                    segment_start,
                    segment_start + display_duration,
                    text,
                    is_sentence_end(current_segment.last().unwrap().2.as_str()),
                    segment_end
                ));

                total_segments += 1;
            } else {
                // Для пустых сегментов сохраняем их как периоды тишины
                debug!(
                    "Empty segment detected: start={:.2}s, end={:.2}s",
                    segment_start,
                    segment_end
                );
                segments.push(Segment::new(
                    segment_start,
                    segment_end,
                    vec![],
                    false,
                    segment_end
                ));
            }

            current_segment.clear();
            word_count = 0;
        }
    }

    // Обработка случая, когда остались слова, но их меньше min_segment_words
    if !current_segment.is_empty() {
        let segment_start = current_segment.first().unwrap().0;
        let segment_end = current_segment.last().unwrap().1;
        let text: Vec<String> = current_segment.iter()
            .map(|(_,_,word)| word.clone())
            .collect();

        let joined_text = text.join(" ");
        let segment_text = joined_text.trim();
        if !segment_text.is_empty() {
            let display_duration = calculate_display_duration(
                word_count,
                segment_end - segment_start,
                None,
                &config,
                total_duration,
                segment_start,
                segment_text,
            );

            debug!(
                "Final segment {}: words={}, duration={:.2}s, text='{}'",
                total_segments + 1,
                word_count,
                display_duration,
                segment_text
            );

            segments.push(Segment::new(
                segment_start,
                segment_start + display_duration,
                text,
                true,
                segment_end
            ));
        }
    }

    info!("Created {} segments from {} words", segments.len(), words.len());

    // Пост-обработка сегментов для обеспечения корректной синхронизации
    let mut total_error = 0.0;
    let mut last_end = 0.0;
    
    for i in 0..segments.len() {
        if i > 0 {
            let prev_end = segments[i-1].end_time;
            let curr_start = segments[i].start_time;
            let min_gap = segments[i-1].min_gap;
            
            // Вычисляем накопленную ошибку
            total_error = prev_end - last_end;
            
            // Корректируем текущий сегмент с учетом накопленной ошибки
            if total_error.abs() > 0.1 { // Если ошибка больше 100мс
                let correction = if total_error > 0.0 {
                    // Если отстаем - пытаемся нагнать
                    (-total_error).max(-0.2) // Максимум 200мс за раз
                } else {
                    // Если спешим - замедляемся
                    (-total_error).min(0.2)  // Максимум 200мс за раз
                };
                
                segments[i].start_time = (curr_start + correction).max(prev_end + min_gap);
                debug!(
                    "Applying sync correction to segment {}: {}s (total error: {}s)",
                    i + 1, correction, total_error
                );
            }
            
            // Проверяем и корректируем интервалы между сегментами
            if segments[i].start_time < prev_end + min_gap {
                segments[i].start_time = prev_end + min_gap;
                debug!(
                    "Adjusted start time of segment {} to maintain minimum gap",
                    i + 1
                );
            }
        }
        
        // Сохраняем конец текущего сегмента для следующей итерации
        last_end = segments[i].end_time;

        // Проверяем, не выходит ли сегмент за пределы общей длительности
        if segments[i].end_time > total_duration {
            debug!(
                "Segment {} exceeds total duration, adjusting end time from {:.3}s to {:.3}s",
                i+1, segments[i].end_time, total_duration
            );
            segments[i].end_time = total_duration;
        }
    }

    // Проверяем финальную синхронизацию
    let final_duration = segments.last().map_or(0.0, |s| s.end_time);
    if (final_duration - total_duration).abs() > 0.1 {
        warn!(
            "Final duration mismatch: expected={:.3}s, actual={:.3}s, difference={:.3}s",
            total_duration, final_duration, final_duration - total_duration
        );
    } else {
        info!(
            "Final synchronization successful: duration={:.3}s, error={:.3}s",
            final_duration, final_duration - total_duration
        );
    }

    // Форматируем сегменты в VTT
    for segment in segments.iter() {
        let start_formatted = format_timestamp(segment.start_time);
        let end_formatted = format_timestamp(segment.end_time);
        
        // Для пустых сегментов (периодов тишины) добавляем только временные метки
        if segment.words.is_empty() {
            vtt_content.push_str(&format!("{} --> {}\n\n", 
                start_formatted, end_formatted));
        } else {
            let text = segment.words.join(" ");
            vtt_content.push_str(&format!("{} --> {}\n{}\n\n", 
                start_formatted, end_formatted, text));
        }
    }

    info!("VTT conversion completed successfully");
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