use crate::errors::AppResult;
use serde::{Serialize, Deserialize};

pub mod audio;
pub mod demucs;
pub mod soundtouch;
pub mod subtitles;
pub mod fragments;
pub mod synchronizer;

#[cfg(test)]
mod tests;

pub use audio::*;
pub use demucs::*;
pub use soundtouch::*;
pub use subtitles::*;
pub use fragments::*;
pub use synchronizer::*;

/// Конфигурация для обработки аудио
#[derive(Debug, Clone)]
pub struct AudioProcessingConfig {
    /// Конфигурация для базовых операций с аудио
    pub audio: AudioConfig,
    /// Конфигурация для Demucs
    pub demucs: DemucsConfig,
    /// Конфигурация для обработки фрагментов
    pub fragments: FragmentProcessingConfig,
}

impl Default for AudioProcessingConfig {
    fn default() -> Self {
        Self {
            audio: AudioConfig::default(),
            demucs: DemucsConfig::default(),
            fragments: FragmentProcessingConfig::default(),
        }
    }
}

/// Проверяет наличие всех необходимых зависимостей
pub fn check_dependencies() -> AppResult<()> {
    ensure_ffmpeg_installed()?;
    ensure_soundtouch_installed()?;
    ensure_demucs_installed()?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DemucsProgress {
    Started,
    Loading,
    Separating(f32),
    Completed,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SoundTouchProgress {
    Started,
    Processing(f32),
    Completed,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FragmentProgress {
    Started,
    Processing(f32),
    Completed,
    Error(String),
} 