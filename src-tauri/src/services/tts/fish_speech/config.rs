use super::models::FishSpeechResult;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::fs;
use once_cell::sync::Lazy;

/// Configuration for Fish Speech
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FishSpeechConfig {
    /// Path to the Fish Speech installation
    pub install_path: PathBuf,
    
    /// Path to store generated audio files
    pub output_path: PathBuf,
    
    /// Whether to use API mode (vs direct model inference)
    pub use_api: bool,
    
    /// API endpoint (for API mode)
    pub api_endpoint: Option<String>,
    
    /// API port (for local API)
    pub api_port: Option<u16>,
    
    /// Default voice ID
    pub default_voice_id: Option<String>,
    
    /// Whether to use GPU for inference
    pub use_gpu: bool,
    
    /// Device to use for inference (e.g., "cuda", "cpu", "mps")
    pub device: String,
    
    /// Maximum audio length in seconds
    pub max_audio_length: f32,
}

impl Default for FishSpeechConfig {
    fn default() -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let install_path = home_dir.join(".fish-speech");
        let output_path = home_dir.join(".fish-speech").join("output");
        
        FishSpeechConfig {
            install_path,
            output_path,
            use_api: true,
            api_endpoint: None,
            api_port: Some(7860),
            default_voice_id: None,
            use_gpu: true,
            device: "auto".to_string(),
            max_audio_length: 60.0,
        }
    }
}

static CONFIG: Lazy<Arc<Mutex<Option<FishSpeechConfig>>>> = Lazy::new(|| {
    Arc::new(Mutex::new(None))
});

const CONFIG_FILENAME: &str = "fish_speech_config.json";

/// Get the path to the config file
fn get_config_path() -> PathBuf {
    let app_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("videonova");
    
    // Ensure the directory exists
    if !app_dir.exists() {
        let _ = fs::create_dir_all(&app_dir);
    }
    
    app_dir.join(CONFIG_FILENAME)
}

/// Save configuration to disk
fn save_config(config: &FishSpeechConfig) -> FishSpeechResult<()> {
    let config_path = get_config_path();
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| crate::fs_tts::models::FishSpeechError::InvalidConfig(e.to_string()))?;
    
    fs::write(config_path, json)
        .map_err(|e| crate::fs_tts::models::FishSpeechError::IoError(e))?;
    
    Ok(())
}

/// Load configuration from disk
fn load_config() -> FishSpeechResult<FishSpeechConfig> {
    let config_path = get_config_path();
    
    if !config_path.exists() {
        let config = FishSpeechConfig::default();
        save_config(&config)?;
        return Ok(config);
    }
    
    let json = fs::read_to_string(config_path)
        .map_err(|e| crate::fs_tts::models::FishSpeechError::IoError(e))?;
    
    let config = serde_json::from_str(&json)
        .map_err(|e| crate::fs_tts::models::FishSpeechError::InvalidConfig(e.to_string()))?;
    
    Ok(config)
}

/// Set Fish Speech configuration
pub fn set_config(config: FishSpeechConfig) -> FishSpeechResult<()> {
    let mut config_guard = CONFIG.lock().unwrap();
    *config_guard = Some(config.clone());
    save_config(&config)
}

/// Get Fish Speech configuration
pub fn get_config() -> FishSpeechResult<FishSpeechConfig> {
    let mut config_guard = CONFIG.lock().unwrap();
    
    if let Some(config) = config_guard.clone() {
        return Ok(config);
    }
    
    // Load config from disk if not in memory
    let config = load_config()?;
    *config_guard = Some(config.clone());
    
    Ok(config)
}

/// Check if Fish Speech is configured
pub fn is_configured() -> bool {
    let config_path = get_config_path();
    config_path.exists()
}

/// Initialize Fish Speech configuration directory
pub fn init_config_dir() -> FishSpeechResult<()> {
    let config = get_config()?;
    
    // Ensure output directory exists
    if !config.output_path.exists() {
        fs::create_dir_all(&config.output_path)
            .map_err(|e| crate::fs_tts::models::FishSpeechError::IoError(e))?;
    }
    
    Ok(())
} 