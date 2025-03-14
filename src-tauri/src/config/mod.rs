// Configuration module
// Centralized management of application configuration

use serde::{Serialize, Deserialize};

pub mod tts;  // TTS configuration

// Other configuration modules will be added as needed

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub openai_api_key: String,
    pub fish_speech_model_path: String,
    pub default_tts_engine: String,
    pub temp_dir: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            openai_api_key: String::new(),
            fish_speech_model_path: "models/fish_speech.onnx".to_string(),
            default_tts_engine: "fishspeech".to_string(),
            temp_dir: "temp".to_string(),
        }
    }
} 