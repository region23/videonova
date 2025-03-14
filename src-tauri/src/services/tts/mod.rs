// TTS services module
// Contains implementations of different TTS engines

use std::path::Path;
use tokio::sync::mpsc::Sender;
use serde::{Serialize, Deserialize};
use crate::services::tts::openai::OpenAiClient;
use crate::services::tts::fishspeech::FishSpeechClient;

pub mod common;
pub mod openai;
pub mod fishspeech;
pub mod tts;

pub use common::*;
pub use openai::{ProgressUpdate as OpenAIProgressUpdate};
pub use fishspeech::{ProgressUpdate as FishSpeechProgressUpdate};
pub use tts::vtt;

use crate::models::{SpeechGenerationRequest, SpeechGenerationResult};
use crate::errors::AppResult;
use crate::config::AppConfig;

/// Обновления о прогрессе генерации речи
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProgressUpdate {
    /// Начало генерации
    Started,
    /// Генерация речи
    GeneratingSpeech(f32),
    /// Генерация речи для сегментов
    TTSGeneration { current: usize, total: usize },
    /// Обработка аудио
    ProcessingAudio(f32),
    /// Завершение генерации
    Completed,
    /// Ошибка
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioProcessingConfig {
    pub window_size: usize,
    pub hop_size: usize,
    pub target_peak_level: f32,
    pub voice_to_instrumental_ratio: f32,
}

impl Default for AudioProcessingConfig {
    fn default() -> Self {
        Self {
            window_size: 2048,
            hop_size: 512,
            target_peak_level: -3.0,
            voice_to_instrumental_ratio: 0.7,
        }
    }
}

/// Trait that all TTS services must implement
#[async_trait::async_trait]
pub trait TtsService: Send + Sync {
    /// Generate speech from text
    async fn generate_speech(
        &self,
        request: &SpeechGenerationRequest,
        audio_config: &AudioProcessingConfig,
        progress_sender: Option<Sender<ProgressUpdate>>,
    ) -> AppResult<SpeechGenerationResult>;
}

/// Get the appropriate TTS service based on the engine name
pub fn get_tts_service(engine: &str, config: &AppConfig) -> AppResult<Box<dyn TtsService>> {
    match engine {
        "openai" => Ok(Box::new(OpenAiClient::new(&config.openai_api_key)?)),
        "fishspeech" => Ok(Box::new(FishSpeechClient::new()?)),
        _ => Err(format!("Unsupported TTS engine: {}", engine).into())
    }
}

/// Get the list of available TTS engines
pub fn get_available_engines() -> Vec<String> {
    vec!["openai".to_string(), "fishspeech".to_string()]
}

/// Get the default engine
pub fn get_default_engine(config: &AppConfig) -> String {
    if !config.openai_api_key.is_empty() {
        "openai".to_string()
    } else {
        "fishspeech".to_string()
    }
} 