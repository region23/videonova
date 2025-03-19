//! # OpenAI TTS Integration
//! 
//! Модуль для взаимодействия с API OpenAI Text-to-Speech.
//! Предоставляет функционал для генерации речи из текста с помощью различных
//! голосовых моделей OpenAI.

use reqwest::{Client, header};
use serde_json::{json, Value};
use serde::Serialize;
use log::{info, warn, error};
use std::time::Duration;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;

use crate::utils::tts::types::{TtsError, Result, TtsVoiceConfig};

// Кеш для хранения уже сгенерированных аудио-фрагментов
static TTS_CACHE: Lazy<Mutex<HashMap<String, Vec<u8>>>> = Lazy::new(|| {
    Mutex::new(HashMap::new())
});

/// Параметры запроса к API OpenAI TTS
#[derive(Debug, Serialize)]
struct TtsRequest<'a> {
    model: &'a str,
    input: &'a str,
    voice: &'a str,
    speed: f32,
    response_format: &'a str,
}

/// Генерирует речь из текста через OpenAI TTS API.
/// 
/// # Аргументы
/// 
/// * `api_key` - Ключ API OpenAI
/// * `text` - Текст для озвучивания
/// * `config` - Конфигурация голоса и модели
/// 
/// # Возвращает
/// 
/// Кортеж из аудио данных в формате MP3 и текста
pub async fn generate_tts(api_key: &str, text: &str, config: &TtsVoiceConfig) -> Result<(Vec<u8>, String)> {
    // Проверяем, есть ли этот фрагмент уже в кеше
    let cache_key = format!("{}:{}:{}:{}", text, config.voice, config.model, config.speed);
    
    // Проверяем кеш
    {
        let cache = TTS_CACHE.lock().unwrap();
        if let Some(cached_audio) = cache.get(&cache_key) {
            info!("Используем кешированный TTS для текста: '{}'", text);
            return Ok((cached_audio.clone(), text.to_string()));
        }
    }
    
    // Подготовка текста перед отправкой
    let processed_text = preprocess_text(text);
    
    // Настройка HTTP клиента с таймаутами и повторными попытками
    let client = Client::builder()
        .timeout(Duration::from_secs(60))  // Увеличиваем таймаут до 60 секунд
        .build()
        .map_err(|e| TtsError::HttpError(e))?;
    
    // Настройка заголовков
    let mut headers = header::HeaderMap::new();
    headers.insert(header::AUTHORIZATION, format!("Bearer {}", api_key).parse().unwrap());
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    
    // Подготовка тела запроса
    let request_body = TtsRequest {
        model: &config.model,
        input: &processed_text,
        voice: &config.voice,
        speed: config.speed,
        response_format: "mp3",
    };
    
    // Отправка запроса с повторными попытками
    let mut attempts = 0;
    let max_attempts = 5;
    
    while attempts < max_attempts {
        info!("Отправка TTS запроса для текста: '{}' (попытка {}/{})", processed_text, attempts + 1, max_attempts);
        
        let response = client.post("https://api.openai.com/v1/audio/speech")
            .headers(headers.clone())
            .json(&request_body)
            .send()
            .await;
            
        match response {
            Ok(resp) => {
                let status = resp.status();
                
                if status.is_success() {
                    // Успешный ответ
                    let audio_data = resp.bytes().await
                        .map_err(|e| TtsError::HttpError(e))?
                        .to_vec();
                    
                    info!("Успешно получен аудио-ответ от API OpenAI TTS: {} байт", audio_data.len());
                    
                    // Кешируем результат
                    {
                        let mut cache = TTS_CACHE.lock().unwrap();
                        cache.insert(cache_key, audio_data.clone());
                    }
                    
                    return Ok((audio_data, processed_text));
                } else {
                    // Обработка ошибки
                    let error_text = resp.text().await.unwrap_or_else(|_| "Не удалось получить текст ошибки".to_string());
                    let error_json: Value = serde_json::from_str(&error_text).unwrap_or_else(|_| json!({"error": {"message": error_text}}));
                    
                    let error_message = error_json["error"]["message"].as_str()
                        .unwrap_or("Неизвестная ошибка API");
                    
                    error!("Ошибка API OpenAI TTS (статус {}): {}", status, error_message);
                    
                    // Проверяем, стоит ли повторить запрос
                    if status.as_u16() == 429 || status.as_u16() >= 500 {
                        attempts += 1;
                        if attempts < max_attempts {
                            // Экспоненциальная задержка
                            let base_delay = 2u64.pow(attempts as u32);
                            // Добавляем фиксированную задержку вместо случайной
                            let wait_time = Duration::from_secs(base_delay) + Duration::from_millis(500);
                            
                            warn!("Повтор запроса через {} секунд (+ 500ms задержки)...", 
                                  base_delay);
                            tokio::time::sleep(wait_time).await;
                            continue;
                        }
                    }
                    
                    return Err(TtsError::OpenAiApiError(format!("Ошибка API ({}): {}", status, error_message)));
                }
            },
            Err(e) => {
                error!("Ошибка HTTP при запросе к API OpenAI TTS: {}", e);
                
                // Повторяем запрос при ошибках сети
                attempts += 1;
                if attempts < max_attempts {
                    // Экспоненциальная задержка
                    let base_delay = 2u64.pow(attempts as u32);
                    // Добавляем фиксированную задержку вместо случайной
                    let wait_time = Duration::from_secs(base_delay) + Duration::from_millis(500);
                    
                    warn!("Повтор запроса через {} секунд (+ 500ms задержки)...", 
                          base_delay);
                    tokio::time::sleep(wait_time).await;
                    continue;
                }
                
                return Err(TtsError::HttpError(e));
            }
        }
    }
    
    Err(TtsError::OpenAiApiError("Превышено максимальное количество попыток".to_string()))
}

/// Генерирует речь для батча текстов через OpenAI TTS API.
/// 
/// # Аргументы
/// 
/// * `api_key` - Ключ API OpenAI
/// * `texts` - Массив текстов для озвучивания
/// * `config` - Конфигурация голоса и модели
/// 
/// # Возвращает
/// 
/// Вектор пар (аудио данные в формате MP3, обработанный текст)
pub async fn generate_tts_batch(
    api_key: &str, 
    texts: &[String], 
    config: &TtsVoiceConfig
) -> Result<Vec<(Vec<u8>, String)>> {
    // Проверяем, не пустой ли массив текстов
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    
    // Результаты для возврата
    let mut results = Vec::with_capacity(texts.len());
    
    // Тексты, которые нужно запросить (не найдены в кеше)
    let mut texts_to_request = Vec::new();
    // Индексы текстов в исходном массиве для сопоставления после запроса
    let mut indices_to_request = Vec::new();
    // Обработанные версии текстов
    let mut processed_texts = Vec::with_capacity(texts.len());
    
    // Проверяем кеш для каждого текста и собираем те, которые нужно запросить
    for (i, text) in texts.iter().enumerate() {
        let processed_text = preprocess_text(text);
        processed_texts.push(processed_text.clone());
        
        let cache_key = format!("{}:{}:{}:{}", text, config.voice, config.model, config.speed);
        
        let cache = TTS_CACHE.lock().unwrap();
        if let Some(cached_audio) = cache.get(&cache_key) {
            info!("Используем кешированный TTS для текста: '{}'", text);
            results.push((cached_audio.clone(), processed_text));
        } else {
            // Этот текст нужно запросить от API
            texts_to_request.push(text.clone());
            indices_to_request.push(i);
        }
    }
    
    // Если все тексты найдены в кеше, возвращаем результаты
    if texts_to_request.is_empty() {
        return Ok(results);
    }
    
    // Устанавливаем максимальный размер батча (количество текстов в одном запросе)
    let batch_size = 5;
    
    // Делим тексты на батчи и отправляем запросы
    for chunk_indices in indices_to_request.chunks(batch_size) {
        // Создаем подмножество текстов для этого батча
        let batch_texts: Vec<String> = chunk_indices.iter()
            .map(|&idx| texts_to_request[idx - indices_to_request[0]].clone())
            .collect();
        
        info!("Обработка батча текстов ({} текстов из {})", batch_texts.len(), texts_to_request.len());
        
        // Обрабатываем каждый текст в батче отдельно
        // В будущем можно оптимизировать для одновременной отправки всех текстов,
        // если API будет поддерживать множественные запросы
        for (j, text) in batch_texts.iter().enumerate() {
            let idx = chunk_indices[j];
            let processed_text = &processed_texts[idx];
            
            info!("Обработка текста в батче {}/{}: '{}'", j + 1, batch_texts.len(), text);
            
            // Генерируем TTS для этого текста
            let cache_key = format!("{}:{}:{}:{}", text, config.voice, config.model, config.speed);
            
            // Настройка HTTP клиента с увеличенным таймаутом
            let client = Client::builder()
                .timeout(Duration::from_secs(60))  // 60 секунд для батча
                .build()
                .map_err(|e| TtsError::HttpError(e))?;
            
            // Настройка заголовков
            let mut headers = header::HeaderMap::new();
            headers.insert(header::AUTHORIZATION, format!("Bearer {}", api_key).parse().unwrap());
            headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
            
            // Подготовка тела запроса
            let request_body = TtsRequest {
                model: &config.model,
                input: processed_text,
                voice: &config.voice,
                speed: config.speed,
                response_format: "mp3",
            };
            
            // Отправка запроса с повторными попытками
            let mut attempts = 0;
            let max_attempts = 5;
            
            let mut audio_data = None;
            
            while attempts < max_attempts && audio_data.is_none() {
                info!("Отправка TTS запроса для текста в батче: '{}' (попытка {}/{})", text, attempts + 1, max_attempts);
                
                let response = client.post("https://api.openai.com/v1/audio/speech")
                    .headers(headers.clone())
                    .json(&request_body)
                    .send()
                    .await;
                    
                match response {
                    Ok(resp) => {
                        let status = resp.status();
                        
                        if status.is_success() {
                            // Успешный ответ
                            match resp.bytes().await {
                                Ok(bytes) => {
                                    let data = bytes.to_vec();
                                    info!("Успешно получен аудио-ответ от API OpenAI TTS: {} байт", data.len());
                                    
                                    // Кешируем результат
                                    {
                                        let mut cache = TTS_CACHE.lock().unwrap();
                                        cache.insert(cache_key.clone(), data.clone());
                                    }
                                    
                                    audio_data = Some(data);
                                },
                                Err(e) => {
                                    error!("Ошибка при чтении ответа API: {}", e);
                                    attempts += 1;
                                    if attempts < max_attempts {
                                        // Экспоненциальная задержка
                                        let base_delay = 2u64.pow(attempts as u32);
                                        // Добавляем фиксированную задержку вместо случайной
                                        let wait_time = Duration::from_secs(base_delay) + Duration::from_millis(500);
                                        
                                        warn!("Повтор запроса через {} секунд (+ 500ms задержки)...", 
                                              base_delay);
                                        tokio::time::sleep(wait_time).await;
                                    }
                                }
                            }
                        } else {
                            // Обработка ошибки
                            let error_text = resp.text().await.unwrap_or_else(|_| "Не удалось получить текст ошибки".to_string());
                            let error_json: Value = serde_json::from_str(&error_text).unwrap_or_else(|_| json!({"error": {"message": error_text}}));
                            
                            let error_message = error_json["error"]["message"].as_str()
                                .unwrap_or("Неизвестная ошибка API");
                            
                            error!("Ошибка API OpenAI TTS (статус {}): {}", status, error_message);
                            
                            // Проверяем, стоит ли повторить запрос
                            if status.as_u16() == 429 || status.as_u16() >= 500 {
                                attempts += 1;
                                if attempts < max_attempts {
                                    // Экспоненциальная задержка
                                    let base_delay = 2u64.pow(attempts as u32);
                                    // Добавляем фиксированную задержку вместо случайной
                                    let wait_time = Duration::from_secs(base_delay) + Duration::from_millis(500);
                                    
                                    warn!("Повтор запроса через {} секунд (+ 500ms задержки)...", 
                                          base_delay);
                                    tokio::time::sleep(wait_time).await;
                                }
                            } else {
                                return Err(TtsError::OpenAiApiError(format!("Ошибка API ({}): {}", status, error_message)));
                            }
                        }
                    },
                    Err(e) => {
                        error!("Ошибка HTTP при запросе к API OpenAI TTS: {}", e);
                        
                        // Повторяем запрос при ошибках сети
                        attempts += 1;
                        if attempts < max_attempts {
                            // Экспоненциальная задержка
                            let base_delay = 2u64.pow(attempts as u32);
                            // Добавляем фиксированную задержку вместо случайной
                            let wait_time = Duration::from_secs(base_delay) + Duration::from_millis(500);
                            
                            warn!("Повтор запроса через {} секунд (+ 500ms задержки)...", 
                                  base_delay);
                            tokio::time::sleep(wait_time).await;
                            continue;
                        } else {
                            return Err(TtsError::HttpError(e));
                        }
                    }
                }
            }
            
            if let Some(data) = audio_data {
                results.push((data, processed_text.clone()));
            } else {
                return Err(TtsError::OpenAiApiError(format!("Не удалось получить аудио для текста: '{}'", text)));
            }
            
            // Добавляем небольшую паузу между запросами в батче, чтобы не перегружать API
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    
    // Сортируем результаты по исходному порядку текстов
    let mut sorted_results = vec![None; texts.len()];
    
    for (i, (audio, text)) in results.into_iter().enumerate() {
        // Найдем индекс этого текста в исходном массиве
        if i < indices_to_request.len() {
            let original_idx = indices_to_request[i];
            sorted_results[original_idx] = Some((audio, text));
        }
    }
    
    // Убедимся, что у нас есть результаты для всех текстов
    Ok(sorted_results.into_iter().flatten().collect())
}

/// Предобрабатывает текст перед отправкой в API TTS.
/// 
/// # Аргументы
/// 
/// * `text` - Исходный текст
/// 
/// # Возвращает
/// 
/// Предобработанный текст
fn preprocess_text(text: &str) -> String {
    let mut result = text.trim().to_string();
    
    // Обработка многоточий
    result = result.replace("...", ". ");
    
    // Удаление дублирующихся пробелов
    while result.contains("  ") {
        result = result.replace("  ", " ");
    }
    
    // Добавляем точку в конце, если отсутствует завершающий знак препинания
    if !result.is_empty() && !".!?".contains(result.chars().last().unwrap()) {
        result.push('.');
    }
    
    // Нормализация переводов строк
    result = result.replace('\n', " ");
    
    result
}

/// Возвращает список доступных голосов TTS.
#[allow(dead_code)]
pub fn available_voices() -> Vec<String> {
    vec![
        "alloy".to_string(),
        "echo".to_string(), 
        "fable".to_string(),
        "onyx".to_string(),
        "nova".to_string(),
        "shimmer".to_string(),
    ]
}

/// Возвращает список доступных моделей TTS.
#[allow(dead_code)]
pub fn available_models() -> Vec<String> {
    vec![
        "tts-1".to_string(),
        "tts-1-hd".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocess_text() {
        assert_eq!(preprocess_text("Hello  world"), "Hello world.");
        assert_eq!(preprocess_text("Hello world!"), "Hello world!");
        assert_eq!(preprocess_text("Line 1\nLine 2"), "Line 1 Line 2.");
        assert_eq!(preprocess_text("Text with...ellipsis"), "Text with. ellipsis.");
    }

    // Тест для мокинга API будет здесь, если это необходимо
} 