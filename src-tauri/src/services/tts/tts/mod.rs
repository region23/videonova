// Inner TTS module
// Contains common TTS functionality that's shared between different engines

use serde::{Serialize, Deserialize};
use tokio::sync::mpsc::Sender;

// Экспортируем подмодули
pub mod vtt;
pub mod synchronizer;

// Экспортируем типы
use crate::errors::AppResult;
use crate::services::tts::common::synchronizer::SyncConfig;

/// Обновление прогресса TTS генерации
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProgressUpdate {
    /// Начало процесса
    Started,
    /// Парсинг VTT файла
    ParsingVTT,
    /// VTT файл разобран
    ParsedVTT { total: usize },
    /// Генерация TTS для сегментов
    TTSGeneration { current: usize, total: usize },
    /// Обработка фрагмента
    ProcessingFragment { index: usize, total: usize, step: String },
    /// Объединение фрагментов
    MergingFragments,
    /// Нормализация аудио
    Normalizing { using_original: bool },
    /// Кодирование результата
    Encoding,
    /// Процесс завершен
    Finished,
    /// Процесс полностью завершен (для совместимости)
    Completed,
}

/// Инициализирует модуль с синхронизатором
pub async fn init() -> AppResult<()> {
    // Пока просто заглушка
    Ok(())
}

/// Обрабатывает синхронизацию аудио и видео
pub async fn process_sync(config: SyncConfig<'_>) -> AppResult<()> {
    // Делегируем обработку в модуль synchronizer
    crate::services::tts::common::synchronizer::process_sync(config).await
} 