use serde::{Serialize, Deserialize};
use tokio::sync::mpsc::Sender;
use crate::errors::AppResult;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeProgress {
    pub progress: f32,
    pub status: String,
}

/// Объединяет видео и аудио файлы
pub async fn merge_files(
    video_path: &Path,
    audio_path: &Path,
    output_path: &Path,
    progress_sender: Option<Sender<MergeProgress>>,
) -> AppResult<String> {
    // Заглушка для функции
    // В реальной реализации здесь будет код для объединения файлов
    
    // Отправляем прогресс
    if let Some(sender) = &progress_sender {
        let _ = sender.send(MergeProgress {
            progress: 1.0,
            status: "Объединение завершено".to_string(),
        }).await;
    }
    
    Ok(output_path.to_string_lossy().to_string())
}
