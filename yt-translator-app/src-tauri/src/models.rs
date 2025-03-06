use serde::{Deserialize, Serialize};

/// Enum representing the process status for the translation pipeline
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProcessStatus {
    /// Process is idle
    Idle,
    /// Downloading video from YouTube
    Downloading,
    /// Recognizing speech from audio
    Recognizing,
    /// Translating subtitles
    Translating,
    /// Generating speech from translated text
    GeneratingSpeech,
    /// Merging audio, video and subtitles
    Merging,
    /// Process completed successfully
    Completed,
    /// Error occurred during processing
    Error,
}

/// Structure representing the progress update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressUpdate {
    /// Current status of the process
    pub status: ProcessStatus,
    /// Progress in percent (0-100)
    pub progress: f32,
    /// Optional message with additional information
    pub message: Option<String>,
    /// Optional path to the output file (when completed)
    pub output_file: Option<String>,
}

/// Structure representing the translation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationRequest {
    /// YouTube URL to download and translate
    pub youtube_url: String,
    /// Source language code (or "auto" for auto-detection)
    pub source_language: String,
    /// Target language code
    pub target_language: String,
    /// Output directory path
    pub output_directory: String,
}

/// Structure representing an error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
} 