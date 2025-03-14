use serde::{Deserialize, Serialize};

/// Модель запроса на генерацию речи
#[derive(Debug, Serialize, Deserialize)]
pub struct SpeechGenerationRequest {
    /// Текст для преобразования в речь
    pub text: String,
    
    /// Выбранный TTS движок
    pub engine: String,
    
    /// Параметры для OpenAI
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai_params: Option<OpenAITtsParams>,
    
    /// Параметры для FishSpeech
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fish_speech_params: Option<FishSpeechParams>,
}

/// Параметры генерации речи OpenAI
#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAITtsParams {
    /// Выбранный голос
    pub voice: String,
    
    /// Модель (по умолчанию tts-1)
    #[serde(default = "default_openai_model")]
    pub model: String,
    
    /// Скорость речи (0.25 - 4.0), по умолчанию 1.0
    #[serde(default = "default_speed")]
    pub speed: f32,
}

fn default_openai_model() -> String {
    "tts-1".to_string()
}

fn default_speed() -> f32 {
    1.0
}

/// Параметры генерации речи FishSpeech
#[derive(Debug, Serialize, Deserialize)]
pub struct FishSpeechParams {
    /// Идентификатор голоса
    pub voice_id: String,
    
    /// Использование GPU для генерации
    #[serde(default = "default_use_gpu")]
    pub use_gpu: bool,
}

fn default_use_gpu() -> bool {
    true
}

/// Результат генерации речи
#[derive(Debug, Serialize, Deserialize)]
pub struct SpeechGenerationResult {
    /// Путь к сгенерированному аудио файлу
    pub audio_path: String,
    
    /// Длительность аудио в секундах
    pub duration: f64,
    
    /// Количество использованных токенов (для OpenAI)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<u32>,
} 