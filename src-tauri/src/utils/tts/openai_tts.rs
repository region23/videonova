//! # OpenAI TTS Integration
//! 
//! Модуль для взаимодействия с API OpenAI Text-to-Speech.
//! Предоставляет функционал для генерации речи из текста с помощью различных
//! голосовых моделей OpenAI.

use reqwest::{Client, header};
use serde_json::{json, Value};
use serde::{Deserialize, Serialize};
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
        .timeout(Duration::from_secs(30))
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
    let max_attempts = 3;
    
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
                            let wait_time = Duration::from_secs(2u64.pow(attempts as u32));
                            warn!("Повтор запроса через {} секунд...", wait_time.as_secs());
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
                    let wait_time = Duration::from_secs(2u64.pow(attempts as u32));
                    warn!("Повтор запроса через {} секунд...", wait_time.as_secs());
                    tokio::time::sleep(wait_time).await;
                    continue;
                }
                
                return Err(TtsError::HttpError(e));
            }
        }
    }
    
    Err(TtsError::OpenAiApiError("Превышено максимальное количество попыток".to_string()))
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