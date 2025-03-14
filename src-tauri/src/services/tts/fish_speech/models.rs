use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Supported audio formats for TTS output
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SpeechFormat {
    Wav,
    Mp3,
    Ogg,
}

impl Default for SpeechFormat {
    fn default() -> Self {
        SpeechFormat::Wav
    }
}

/// Voice model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voice {
    /// Unique identifier for the voice
    pub id: String,
    
    /// Display name for the voice
    pub name: String,
    
    /// Language code (e.g., "en", "ru", "zh")
    pub language: String,
    
    /// Description or additional metadata
    pub description: Option<String>,
    
    /// Whether this is a locally fine-tuned voice
    pub is_fine_tuned: bool,
}

/// Request to generate speech
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsRequest {
    /// Text to convert to speech
    pub text: String,
    
    /// Voice ID to use
    pub voice_id: String,
    
    /// Output format
    #[serde(default)]
    pub format: SpeechFormat,
    
    /// Speech rate (1.0 is normal speed)
    #[serde(default = "default_speech_rate")]
    pub rate: f32,
    
    /// Whether to use streaming response
    #[serde(default)]
    pub stream: bool,
}

fn default_speech_rate() -> f32 {
    1.0
}

/// Response from speech generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsResponse {
    /// Path to the generated audio file
    pub audio_path: PathBuf,
    
    /// Duration of the generated audio in seconds
    pub duration: f32,
    
    /// Format of the generated audio
    pub format: SpeechFormat,
    
    /// Timestamp when generation was completed
    pub timestamp: i64,
}

/// Installation status of Fish Speech
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationStatus {
    /// Whether Fish Speech is installed
    pub installed: bool,
    
    /// Installation path
    pub path: Option<PathBuf>,
    
    /// Installed version
    pub version: Option<String>,
    
    /// Installation progress (0.0 - 1.0)
    pub progress: f32,
    
    /// Current installation step message
    pub status_message: String,
}

/// Error types for Fish Speech operations
#[derive(Debug, thiserror::Error)]
pub enum FishSpeechError {
    #[error("Failed to install Fish Speech: {0}")]
    InstallationError(String),
    
    #[error("Failed to generate speech: {0}")]
    GenerationError(String),
    
    #[error("Failed to process audio: {0}")]
    AudioProcessingError(String),
    
    #[error("Fish Speech is not installed")]
    NotInstalled,
    
    #[error("Fish Speech is not configured")]
    NotConfigured,
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Unknown error: {0}")]
    Unknown(String),
}

// Type alias for Result with FishSpeechError
pub type FishSpeechResult<T> = Result<T, FishSpeechError>; 