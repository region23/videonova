// Error handling module
// Contains custom error types and error handling utilities

use std::fmt;
use serde::Serialize;
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

// Application error type
#[derive(Debug, Error, Serialize)]
pub enum AppError {
    #[error("Ошибка конфигурации: {0}")]
    ConfigurationError(String),
    
    #[error("Ошибка обработки аудио: {0}")]
    AudioProcessingError(String),
    
    #[error("Ошибка API: {0}")]
    ApiError(String),
    
    #[error("Ошибка ввода/вывода: {0}")]
    #[serde(serialize_with = "serialize_io_error")]
    IoError(#[from] std::io::Error),
    
    #[error("Ошибка установки: {0}")]
    InstallationError(String),
    
    #[error("Неизвестная ошибка: {0}")]
    Unknown(String),
    
    #[error("Другая ошибка: {0}")]
    Other(String),
    
    #[error("Ошибка сериализации: {0}")]
    SerializationError(String),
    
    #[error(transparent)]
    #[serde(skip)]
    AnyhowError(#[from] anyhow::Error),
}

// Функция для сериализации std::io::Error, которая не реализует serde::Serialize
fn serialize_io_error<S>(err: &std::io::Error, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&err.to_string())
}

// Реализация трейтов From для различных типов ошибок
impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::ApiError(err.to_string())
    }
}

impl From<String> for AppError {
    fn from(err: String) -> Self {
        AppError::Other(err)
    }
}

impl From<&str> for AppError {
    fn from(err: &str) -> Self {
        AppError::Other(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::SerializationError(err.to_string())
    }
}

// Реализация From для SendError различных типов сообщений
impl<T> From<SendError<T>> for AppError {
    fn from(err: SendError<T>) -> Self {
        AppError::Other(format!("Failed to send message: {}", err))
    }
}

// Result type alias for application
pub type AppResult<T> = Result<T, AppError>; 