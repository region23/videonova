use std::path::Path;
use log::{info, error};
use crate::errors::AppResult;
use crate::services::tts::common::synchronizer::SyncConfig;

/// Обрабатывает синхронизацию аудио и видео
pub async fn process_sync(config: SyncConfig<'_>) -> AppResult<()> {
    info!("Starting TTS synchronization process");
    
    // Делегируем работу на модуль common/synchronizer
    crate::services::tts::common::synchronizer::process_sync(config).await
} 