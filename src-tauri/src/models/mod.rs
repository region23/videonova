// Domain models module
// Contains core data structures used throughout the application

// Reexport all model types for easy access
pub mod tts;

// Экспортируем основные типы для удобства использования
pub use tts::{
    SpeechGenerationRequest as TtsSpeechGenerationRequest,
    SpeechGenerationResult as TtsSpeechGenerationResult,
    OpenAITtsParams as TtsOpenAITtsParams,
    FishSpeechParams as TtsFishSpeechParams,
};

use std::path::PathBuf;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechGenerationRequest {
    pub text: String,
    pub output_path: PathBuf,
    pub engine: String,
    pub remove_vocals: bool,
    pub adjust_pitch: f32,
    pub mix_with_instrumental: bool,
    pub voice_to_instrumental_ratio: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechGenerationResult {
    pub output_path: PathBuf,
    pub duration: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiTtsParams {
    pub model: String,
    pub voice: String,
    pub speed: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FishSpeechParams {
    pub model_path: String,
    pub speaker: String,
    pub speed: f32,
} 