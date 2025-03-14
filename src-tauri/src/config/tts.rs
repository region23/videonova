use serde::{Deserialize, Serialize};
use tauri_plugin_store::Store;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use crate::errors::{AppError, AppResult};

// Доступные движки TTS
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TtsEngine {
    OpenAI,
    FishSpeech,
}

impl Default for TtsEngine {
    fn default() -> Self {
        TtsEngine::OpenAI
    }
}

// Конфигурация TTS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    // Выбранный движок
    pub engine: TtsEngine,
    
    // OpenAI настройки
    pub openai_voice: Option<String>,
    
    // Fish Speech настройки
    pub fish_speech_voice_id: Option<String>,
    pub fish_speech_use_gpu: bool,
}

impl Default for TtsConfig {
    fn default() -> Self {
        TtsConfig {
            engine: TtsEngine::default(),
            openai_voice: Some("alloy".to_string()), // Default OpenAI voice
            fish_speech_voice_id: None,
            fish_speech_use_gpu: true,
        }
    }
}

// Константы для хранения
const STORE_KEY: &str = "tts_config";

// Кэш для конфигурации
static CONFIG_CACHE: Lazy<Mutex<Option<TtsConfig>>> = Lazy::new(|| {
    Mutex::new(None)
});

// Загрузка конфигурации из хранилища
pub async fn load_config(store: &Store<tauri::Wry>) -> AppResult<TtsConfig> {
    let mut cache = CONFIG_CACHE.lock().map_err(|e| 
        AppError::ConfigurationError(format!("Failed to acquire lock for TTS config: {}", e)))?;
    
    // Если конфигурация уже загружена в кэш, вернем ее
    if let Some(config) = &*cache {
        return Ok(config.clone());
    }
    
    // Иначе загрузим из хранилища
    match store.get(STORE_KEY) {
        Some(value) => {
            match serde_json::from_value::<TtsConfig>(value.clone()) {
                Ok(config) => {
                    // Сохраним в кэш
                    *cache = Some(config.clone());
                    Ok(config)
                },
                Err(e) => Err(AppError::ConfigurationError(format!("Failed to parse TTS config: {}", e)))
            }
        },
        None => {
            // Если конфигурации нет, создадим новую с дефолтными значениями
            let config = TtsConfig::default();
            *cache = Some(config.clone());
            Ok(config)
        }
    }
}

// Сохранение конфигурации в хранилище
pub async fn save_config(store: &Store<tauri::Wry>, config: TtsConfig) -> AppResult<()> {
    // Обновим кэш
    {
        let mut cache = CONFIG_CACHE.lock().map_err(|e| 
            AppError::ConfigurationError(format!("Failed to acquire lock for TTS config: {}", e)))?;
        *cache = Some(config.clone());
    }
    
    // Сохраним в хранилище
    match serde_json::to_value(config) {
        Ok(value) => {
            store.set(STORE_KEY, value);
            
            store.save()
                .map_err(|e| AppError::ConfigurationError(format!("Failed to persist TTS config: {}", e)))?;
            
            Ok(())
        },
        Err(e) => Err(AppError::ConfigurationError(format!("Failed to serialize TTS config: {}", e)))
    }
}

// Получение движка по умолчанию (проверка доступности Fish Speech)
pub async fn get_default_engine() -> TtsEngine {
    // В тестовых целях вернем OpenAI как безопасный вариант
    // В будущем здесь может быть логика проверки доступности FishSpeech
    TtsEngine::OpenAI
} 