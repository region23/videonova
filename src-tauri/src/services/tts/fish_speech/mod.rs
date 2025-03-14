// Fish Speech TTS service implementation
// Handles integration with Fish Speech for text-to-speech capabilities

mod installer;
mod api;
mod config;
mod audio;
mod models;

use std::sync::Once;
use crate::models::tts::{SpeechGenerationRequest, SpeechGenerationResult, FishSpeechParams};
use crate::errors::{AppError, AppResult};
use crate::services::tts::TtsService;

static INIT: Once = Once::new();

/// Fish Speech TTS service implementation
pub struct FishSpeechService {
    // Add any Fish Speech-specific state here
}

impl FishSpeechService {
    /// Create a new Fish Speech service instance
    pub fn new() -> Self {
        Self {}
    }

    /// Initialize the Fish Speech service
    pub async fn initialize() -> AppResult<()> {
        INIT.call_once(|| {
            log::info!("Initializing Fish Speech TTS service");
        });
        
        // Check if Fish Speech is installed
        if !installer::is_installed() {
            return Err(AppError::TtsError(
                "Fish Speech is not installed. Call install_fish_speech() first.".to_string()
            ));
        }
        
        // Check if Fish Speech is properly configured
        if !config::is_configured() {
            return Err(AppError::TtsError(
                "Fish Speech is not configured. Call set_config() first.".to_string()
            ));
        }
        
        Ok(())
    }

    /// Get Fish Speech version
    pub fn version() -> String {
        // This would be implemented to query the installed Fish Speech version
        // For now just return a placeholder
        "1.5.0".to_string()
    }
}

impl TtsService for FishSpeechService {
    async fn generate_speech(&self, request: &SpeechGenerationRequest) -> AppResult<SpeechGenerationResult> {
        // Extract Fish Speech specific parameters
        let params = request.fish_speech_params.as_ref()
            .ok_or_else(|| AppError::TtsError("Fish Speech parameters not provided".to_string()))?;

        // TODO: Implement actual speech generation using the Fish Speech API
        Err(AppError::TtsError("Fish Speech TTS not implemented yet".to_string()))
    }

    fn is_ready(&self) -> bool {
        installer::is_installed() && config::is_configured()
    }
}

// Re-export necessary functions and types
pub use installer::install_fish_speech;
pub use api::{generate_speech, list_voices, stop_generation};
pub use config::{FishSpeechConfig, set_config, get_config};
pub use models::{TtsRequest, TtsResponse, Voice, SpeechFormat}; 