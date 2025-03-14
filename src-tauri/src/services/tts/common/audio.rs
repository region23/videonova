use std::path::Path;
use std::process::Command;
use tokio::sync::mpsc::Sender;
use log::{info, warn, error};
use crate::errors::{AppError, AppResult};
use crate::services::tts::demucs::DemucsProgress;
use crate::services::tts::SoundTouchProgress;

/// Конфигурация для обработки аудио
#[derive(Debug, Clone)]
pub struct AudioConfig {
    /// Целевой уровень громкости (в dB)
    pub target_peak_level: f32,
    /// Размер окна для нормализации (в секундах)
    pub window_size: f32,
    /// Размер перекрытия окон (в секундах)
    pub hop_size: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            target_peak_level: -14.0,
            window_size: 0.1,
            hop_size: 0.05,
        }
    }
}

/// Проверяет, установлен ли ffmpeg
pub fn is_ffmpeg_installed() -> bool {
    let output = Command::new("ffmpeg")
        .arg("-version")
        .output();
    
    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Устанавливает ffmpeg
pub fn install_ffmpeg() -> AppResult<()> {
    info!("Установка ffmpeg...");
    
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("brew")
            .args(&["install", "ffmpeg"])
            .status()
            .map_err(|e| AppError::InstallationError(format!("Ошибка установки ffmpeg через Homebrew: {}", e)))?;
            
        if !status.success() {
            return Err(AppError::InstallationError("Не удалось установить ffmpeg через Homebrew".to_string()));
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        let apt_status = Command::new("apt-get")
            .args(&["install", "-y", "ffmpeg"])
            .status();
            
        if let Ok(status) = apt_status {
            if status.success() {
                return Ok(());
            }
        }
        
        let pacman_status = Command::new("pacman")
            .args(&["-S", "--noconfirm", "ffmpeg"])
            .status();
            
        if let Ok(status) = pacman_status {
            if status.success() {
                return Ok(());
            }
        }
        
        return Err(AppError::InstallationError("Не удалось установить ffmpeg. Пожалуйста, установите вручную".to_string()));
    }
    
    #[cfg(target_os = "windows")]
    {
        error!("Автоматическая установка ffmpeg на Windows не поддерживается");
        return Err(AppError::InstallationError("Пожалуйста, скачайте и установите ffmpeg вручную с официального сайта".to_string()));
    }
    
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        return Err(AppError::InstallationError("Автоматическая установка ffmpeg не поддерживается для данной ОС".to_string()));
    }
    
    info!("ffmpeg успешно установлен");
    Ok(())
}

/// Проверяет, установлен ли ffmpeg, и устанавливает его при необходимости
pub fn ensure_ffmpeg_installed() -> AppResult<()> {
    if !is_ffmpeg_installed() {
        info!("ffmpeg не установлен, начинаем установку...");
        install_ffmpeg()?;
    } else {
        info!("ffmpeg уже установлен");
    }
    Ok(())
}

/// Нормализует громкость аудио
pub fn normalize_audio(
    input_path: &Path,
    output_path: &Path,
    config: &AudioConfig,
) -> AppResult<()> {
    ensure_ffmpeg_installed()?;
    
    let status = Command::new("ffmpeg")
        .args(&[
            "-i", input_path.to_str().unwrap(),
            "-filter:a", &format!(
                "loudnorm=I={}:LRA=11:TP=-1.5:measured_I={}:measured_LRA=11:measured_TP=-1.5:measured_thresh=-30:offset=0:linear=true:print_format=json",
                config.target_peak_level,
                config.target_peak_level
            ),
            "-ar", "44100",
            output_path.to_str().unwrap(),
        ])
        .status()
        .map_err(|e| AppError::AudioProcessingError(format!("Ошибка нормализации аудио: {}", e)))?;
        
    if !status.success() {
        return Err(AppError::AudioProcessingError("ffmpeg завершился с ошибкой".to_string()));
    }
    
    Ok(())
}

/// Конвертирует аудио в WAV формат
pub fn convert_to_wav(
    input_path: &Path,
    output_path: &Path,
) -> AppResult<()> {
    ensure_ffmpeg_installed()?;
    
    let status = Command::new("ffmpeg")
        .args(&[
            "-i", input_path.to_str().unwrap(),
            "-acodec", "pcm_s16le",
            "-ar", "44100",
            "-ac", "2",
            output_path.to_str().unwrap(),
        ])
        .status()
        .map_err(|e| AppError::AudioProcessingError(format!("Ошибка конвертации в WAV: {}", e)))?;
        
    if !status.success() {
        return Err(AppError::AudioProcessingError("ffmpeg завершился с ошибкой".to_string()));
    }
    
    Ok(())
}

/// Обрезает аудио до указанной длительности
pub fn trim_audio(
    input_path: &Path,
    output_path: &Path,
    start_time: f32,
    duration: f32,
) -> AppResult<()> {
    ensure_ffmpeg_installed()?;
    
    let status = Command::new("ffmpeg")
        .args(&[
            "-i", input_path.to_str().unwrap(),
            "-ss", &start_time.to_string(),
            "-t", &duration.to_string(),
            "-c", "copy",
            output_path.to_str().unwrap(),
        ])
        .status()
        .map_err(|e| AppError::AudioProcessingError(format!("Ошибка обрезки аудио: {}", e)))?;
        
    if !status.success() {
        return Err(AppError::AudioProcessingError("ffmpeg завершился с ошибкой".to_string()));
    }
    
    Ok(())
}

pub async fn remove_vocals(
    input_path: &Path,
    output_path: &Path,
    progress_sender: Option<Sender<DemucsProgress>>,
) -> AppResult<()> {
    info!("Removing vocals from {}", input_path.display());
    
    if let Some(sender) = &progress_sender {
        sender.send(DemucsProgress::Started).await?;
    }
    
    // TODO: Implement vocal removal using Demucs
    
    if let Some(sender) = &progress_sender {
        sender.send(DemucsProgress::Completed).await?;
    }
    
    Ok(())
}

pub async fn adjust_pitch(
    input_path: &Path,
    output_path: &Path,
    pitch_factor: f32,
    progress_sender: Option<Sender<SoundTouchProgress>>,
) -> AppResult<()> {
    info!("Adjusting pitch by factor {}", pitch_factor);
    
    if let Some(sender) = &progress_sender {
        sender.send(SoundTouchProgress::Started).await?;
    }
    
    // TODO: Implement pitch adjustment using SoundTouch
    
    if let Some(sender) = &progress_sender {
        sender.send(SoundTouchProgress::Completed).await?;
    }
    
    Ok(())
}

pub async fn mix_audio_tracks(
    voice_path: &Path,
    instrumental_path: &Path,
    output_path: &Path,
    voice_to_instrumental_ratio: f32,
) -> AppResult<()> {
    info!("Mixing audio tracks with ratio {}", voice_to_instrumental_ratio);
    
    // TODO: Implement audio mixing
    
    Ok(())
}

pub async fn detect_voice_gender(audio_path: &Path) -> AppResult<String> {
    info!("Detecting voice gender from {}", audio_path.display());
    
    // TODO: Implement voice gender detection using the Python script
    
    Ok("male".to_string()) // Default to male for now
} 