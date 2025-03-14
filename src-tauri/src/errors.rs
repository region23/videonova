use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Ошибка установки: {0}")]
    InstallationError(String),

    #[error("Ошибка обработки аудио: {0}")]
    AudioProcessingError(String),

    #[error("Ошибка API: {0}")]
    ApiError(String),

    #[error("Ошибка конфигурации: {0}")]
    ConfigurationError(String),

    #[error("Ошибка ввода/вывода: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Неизвестная ошибка: {0}")]
    Unknown(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>; 