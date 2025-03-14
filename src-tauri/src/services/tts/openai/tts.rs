use std::path::Path;
use reqwest::Client;
use tokio::sync::mpsc::Sender;
use crate::errors::{AppError, AppResult};
use crate::services::tts::common::*;
use super::{OpenAiConfig, SpeechRequest, ProgressUpdate};
use serde_json::json;
use log::{info, warn, error};
use crate::models::{SpeechGenerationRequest, SpeechGenerationResult};
use crate::services::tts::{AudioProcessingConfig, remove_vocals, adjust_pitch, mix_audio_tracks};

/// Клиент для работы с OpenAI TTS API
pub struct OpenAiClient {
    client: Client,
    api_key: String,
}

impl OpenAiClient {
    /// Создает новый клиент OpenAI TTS
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    /// Генерирует речь с помощью OpenAI TTS API
    pub async fn generate_speech(
        &self,
        request: &SpeechRequest,
        config: &OpenAiConfig,
        progress_sender: Option<Sender<ProgressUpdate>>,
    ) -> AppResult<()> {
        info!("Generating speech using OpenAI TTS");
        
        if let Some(sender) = &progress_sender {
            sender.send(ProgressUpdate::Started).await?;
        }
        
        let response = self.client
            .post("https://api.openai.com/v1/audio/speech")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "model": config.model,
                "voice": config.voice,
                "input": request.text,
                "speed": config.speed,
            }))
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error = response.text().await?;
            if let Some(sender) = &progress_sender {
                sender.send(ProgressUpdate::Error(error.clone())).await?;
            }
            return Err(error.into());
        }
        
        let temp_path = Path::new("temp.wav");
        let bytes = response.bytes().await?;
        tokio::fs::write(temp_path, bytes).await?;
        
        if let Some(sender) = &progress_sender {
            sender.send(ProgressUpdate::GeneratingSpeech(1.0)).await?;
        }
        
        // Process audio if needed
        if request.remove_vocals {
            let vocals_path = Path::new("vocals.wav");
            remove_vocals(temp_path, vocals_path, None).await?;
            
            if request.adjust_pitch != 0.0 {
                let pitched_path = Path::new("pitched.wav");
                adjust_pitch(vocals_path, pitched_path, request.adjust_pitch, None).await?;
                
                if request.mix_with_background {
                    let instrumental_path = Path::new("instrumental.wav");
                    let output_path = Path::new(&request.output_path);
                    mix_audio_tracks(
                        pitched_path,
                        instrumental_path,
                        output_path,
                        request.voice_to_music_ratio,
                    ).await?;
                } else {
                    tokio::fs::rename(pitched_path, &request.output_path).await?;
                }
            } else if request.mix_with_background {
                let instrumental_path = Path::new("instrumental.wav");
                let output_path = Path::new(&request.output_path);
                mix_audio_tracks(
                    vocals_path,
                    instrumental_path,
                    output_path,
                    request.voice_to_music_ratio,
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
        
        Ok(())
    }

    pub async fn validate_api_key(api_key: &str) -> AppResult<bool> {
        let client = Client::new();
        let response = client
            .post("https://api.openai.com/v1/audio/speech")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&json!({
                "model": "tts-1",
                "input": "Test",
                "voice": "alloy",
            }))
            .send()
            .await?;
        
        Ok(response.status().is_success())
    }
} 