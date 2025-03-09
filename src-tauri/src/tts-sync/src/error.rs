//! Модуль обработки ошибок библиотеки tts-sync
//! 
//! Этот модуль содержит типы ошибок, которые могут возникнуть при работе библиотеки.

use thiserror::Error;

/// Ошибки библиотеки tts-sync
#[derive(Debug, Error)]
pub enum TtsSyncError {
    /// Ошибка HTTP запроса
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
    
    /// Ошибка ввода-вывода
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    /// Ошибка сериализации/десериализации JSON
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    /// Ошибка парсинга субтитров
    #[error("Subtitle parsing error: {0}")]
    SubtitleParsing(String),
    
    /// Ошибка обработки субтитров
    #[error("Subtitle processing error: {0}")]
    SubtitleProcessing(String),
    
    /// Ошибка генерации TTS
    #[error("TTS generation error: {0}")]
    TtsGeneration(String),
    
    /// Ошибка обработки аудио
    #[error("Audio processing error: {0}")]
    AudioProcessing(String),
    
    /// Ошибка обработки видео
    #[error("Video processing error: {0}")]
    VideoProcessing(String),
    
    /// Ошибка конфигурации
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    /// Файл не найден
    #[error("File not found: {0}")]
    FileNotFound(String),
    
    /// Неверный формат
    #[error("Invalid format: {0}")]
    InvalidFormat(String),
    
    /// Ошибка синхронизации
    #[error("Synchronization error: {0}")]
    Synchronization(String),
    
    /// Другая ошибка
    #[error("Other error: {0}")]
    Other(String),
}

impl From<&str> for TtsSyncError {
    fn from(s: &str) -> Self {
        TtsSyncError::Other(s.to_string())
    }
}

impl From<String> for TtsSyncError {
    fn from(s: String) -> Self {
        TtsSyncError::Other(s)
    }
}

/// Тип Result для библиотеки tts-sync
pub type Result<T> = std::result::Result<T, TtsSyncError>;
