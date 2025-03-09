//! Модуль конфигурации библиотеки tts-sync
//! 
//! Этот модуль содержит структуры и перечисления для настройки библиотеки.

use serde::{Deserialize, Serialize};

/// Модель TTS для использования с OpenAI API
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TtsModel {
    /// Стандартная модель
    Standard,
    /// Модель высокого качества
    HighDefinition,
}

impl Default for TtsModel {
    fn default() -> Self {
        Self::Standard
    }
}

impl TtsModel {
    /// Получить строковое представление модели
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Standard => "tts-1",
            Self::HighDefinition => "tts-1-hd",
        }
    }
}

/// Голос для использования с OpenAI API
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TtsVoice {
    /// Голос Alloy
    Alloy,
    /// Голос Echo
    Echo,
    /// Голос Fable
    Fable,
    /// Голос Onyx
    Onyx,
    /// Голос Nova
    Nova,
    /// Голос Shimmer
    Shimmer,
}

impl Default for TtsVoice {
    fn default() -> Self {
        Self::Nova
    }
}

impl TtsVoice {
    /// Получить строковое представление голоса
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Alloy => "alloy",
            Self::Echo => "echo",
            Self::Fable => "fable",
            Self::Onyx => "onyx",
            Self::Nova => "nova",
            Self::Shimmer => "shimmer",
        }
    }
}

/// Метод синхронизации
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyncMethod {
    /// Простая синхронизация (без изменения темпа)
    Simple,
    /// Адаптивная синхронизация (с изменением темпа)
    Adaptive,
    /// Автоматический выбор метода
    Auto,
}

impl Default for SyncMethod {
    fn default() -> Self {
        Self::Auto
    }
}

/// Конфигурация библиотеки
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsSyncConfig {
    /// API ключ для OpenAI
    pub openai_api_key: String,
    /// Модель TTS
    pub tts_model: TtsModel,
    /// Голос TTS
    pub tts_voice: TtsVoice,
    /// Громкость оригинального аудио (0.0 - 1.0)
    pub original_audio_volume: f32,
    /// Громкость TTS аудио (0.0 - 1.0)
    pub tts_audio_volume: f32,
    /// Метод синхронизации
    pub sync_method: SyncMethod,
    /// Максимальное количество одновременных запросов к API
    pub max_concurrent_requests: usize,
    /// Использовать кэширование
    pub use_caching: bool,
    /// Директория для кэша
    pub cache_dir: Option<String>,
    /// Максимальный размер кэша в байтах
    pub max_cache_size: Option<u64>,
    /// Удалять временные файлы после завершения
    pub cleanup_temp_files: bool,
}

impl Default for TtsSyncConfig {
    fn default() -> Self {
        Self {
            openai_api_key: String::new(),
            tts_model: TtsModel::default(),
            tts_voice: TtsVoice::default(),
            original_audio_volume: 0.2,
            tts_audio_volume: 1.0,
            sync_method: SyncMethod::default(),
            max_concurrent_requests: 5,
            use_caching: true,
            cache_dir: None,
            max_cache_size: Some(1024 * 1024 * 1024), // 1 GB
            cleanup_temp_files: true,
        }
    }
}
