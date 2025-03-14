use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::sync::mpsc::Sender;
use log::{info, warn, error};
use crate::errors::{AppError, AppResult};
use crate::services::tts::common::*;
use crate::services::tts::TtsService;
use super::{FishSpeechConfig, ProgressUpdate};
use crate::models::{SpeechGenerationRequest, SpeechGenerationResult};
use crate::services::tts::{AudioProcessingConfig, remove_vocals, adjust_pitch, mix_audio_tracks, ProgressUpdate as TtsProgressUpdate};
use crate::services::tts::openai::SpeechRequest;

/// Клиент для работы с Fish Speech
pub struct FishSpeechClient {
    config: FishSpeechConfig,
    model_path: PathBuf,
}

impl FishSpeechClient {
    /// Создает новый клиент Fish Speech
    pub fn new() -> AppResult<Self> {
        let model_path = "models/fish_speech.onnx".to_string();
        Ok(Self {
            config: FishSpeechConfig {
                model_path: model_path.clone(),
                ..Default::default()
            },
            model_path: PathBuf::from(model_path),
        })
    }

    /// Проверяет наличие зависимостей для Fish Speech
    fn check_fish_speech_deps() -> AppResult<()> {
        // Проверяем наличие Python
        let python_status = Command::new("python3")
            .arg("--version")
            .status()
            .map_err(|_| AppError::InstallationError("Python3 не установлен".to_string()))?;

        if !python_status.success() {
            return Err(AppError::InstallationError("Python3 не установлен".to_string()));
        }

        // Проверяем наличие PyTorch
        let torch_check = Command::new("python3")
            .args(&["-c", "import torch"])
            .status()
            .map_err(|_| AppError::InstallationError("PyTorch не установлен".to_string()))?;

        if !torch_check.success() {
            return Err(AppError::InstallationError("PyTorch не установлен".to_string()));
        }

        Ok(())
    }

    /// Устанавливает зависимости для Fish Speech
    fn install_fish_speech_deps() -> AppResult<()> {
        info!("Установка зависимостей Fish Speech...");

        // Устанавливаем PyTorch
        let torch_install = Command::new("pip3")
            .args(&["install", "torch", "torchaudio"])
            .status()
            .map_err(|e| AppError::InstallationError(format!("Ошибка установки PyTorch: {}", e)))?;

        if !torch_install.success() {
            return Err(AppError::InstallationError("Не удалось установить PyTorch".to_string()));
        }

        // Устанавливаем дополнительные зависимости
        let deps_install = Command::new("pip3")
            .args(&["install", "numpy", "soundfile", "librosa"])
            .status()
            .map_err(|e| AppError::InstallationError(format!("Ошибка установки зависимостей: {}", e)))?;

        if !deps_install.success() {
            return Err(AppError::InstallationError("Не удалось установить зависимости".to_string()));
        }

        Ok(())
    }

    /// Проверяет и устанавливает зависимости
    fn ensure_fish_speech_deps() -> AppResult<()> {
        if let Err(_) = Self::check_fish_speech_deps() {
            info!("Зависимости Fish Speech не установлены, начинаем установку...");
            Self::install_fish_speech_deps()?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl TtsService for FishSpeechClient {
    async fn generate_speech(
        &self,
        request: &SpeechGenerationRequest,
        audio_config: &AudioProcessingConfig,
        progress_sender: Option<Sender<TtsProgressUpdate>>,
    ) -> AppResult<SpeechGenerationResult> {
        info!("Generating speech using FishSpeech");
        
        if let Some(sender) = &progress_sender {
            sender.send(TtsProgressUpdate::Started).await?;
        }
        
        // Load model
        if let Some(sender) = &progress_sender {
            sender.send(TtsProgressUpdate::GeneratingSpeech(0.1)).await?;
        }
        
        // TODO: Implement FishSpeech TTS generation
        
        if let Some(sender) = &progress_sender {
            sender.send(TtsProgressUpdate::Completed).await?;
        }
        
        Ok(SpeechGenerationResult {
            output_path: request.output_path.clone(),
            duration: 0.0,
        })
    }
}

pub async fn generate_speech(
    model_path: &str,
    request: &SpeechGenerationRequest,
    config: &FishSpeechConfig,
    audio_config: &AudioProcessingConfig,
    progress_sender: Option<Sender<ProgressUpdate>>,
) -> AppResult<SpeechGenerationResult> {
    info!("Generating speech using FishSpeech");
    
    if let Some(sender) = &progress_sender {
        sender.send(ProgressUpdate::Started).await?;
    }
    
    // Load model
    if let Some(sender) = &progress_sender {
        sender.send(ProgressUpdate::LoadingModel).await?;
    }
    
    // TODO: Implement FishSpeech TTS generation
    // 1. Load ONNX model
    // 2. Generate speech
    // 3. Process audio
    
    let temp_path = Path::new("temp.wav");
    
    // Process audio if needed
    if request.remove_vocals {
        let vocals_path = Path::new("vocals.wav");
        remove_vocals(temp_path, vocals_path, None).await?;
        
        if request.adjust_pitch != 0.0 {
            let pitched_path = Path::new("pitched.wav");
            adjust_pitch(vocals_path, pitched_path, request.adjust_pitch, None).await?;
            
            if request.mix_with_instrumental {
                let instrumental_path = Path::new("instrumental.wav");
                mix_audio_tracks(
                    pitched_path,
                    instrumental_path,
                    &request.output_path,
                    audio_config.voice_to_instrumental_ratio,
                ).await?;
            } else {
                tokio::fs::rename(pitched_path, &request.output_path).await?;
            }
        } else if request.mix_with_instrumental {
            let instrumental_path = Path::new("instrumental.wav");
            mix_audio_tracks(
                vocals_path,
                instrumental_path,
                &request.output_path,
                audio_config.voice_to_instrumental_ratio,
            ).await?;
        } else {
            tokio::fs::rename(vocals_path, &request.output_path).await?;
        }
    } else {
        tokio::fs::rename(temp_path, &request.output_path).await?;
    }
    
    if let Some(sender) = &progress_sender {
        sender.send(ProgressUpdate::Completed).await?;
    }
    
    Ok(SpeechGenerationResult {
        output_path: request.output_path.clone(),
        duration: 0.0, // TODO: Calculate actual duration
    })
}

pub async fn check_model_exists(model_path: &str) -> bool {
    Path::new(model_path).exists()
} 