use std::path::Path;
use std::process::Command;
use tokio::process::Command as TokioCommand;
use tokio::io::{BufReader, AsyncBufReadExt};
use log::{info, warn, error};
use crate::errors::{AppError, AppResult};
use super::audio::ensure_ffmpeg_installed;
use tokio::sync::mpsc::Sender;

/// Конфигурация для Demucs
#[derive(Debug, Clone)]
pub struct DemucsConfig {
    /// Путь к модели Demucs
    pub model_path: String,
    /// Размер сегмента для обработки (в секундах)
    pub segment_size: f32,
    /// Перекрытие между сегментами (в секундах)
    pub overlap: f32,
    /// Использовать GPU если доступно
    pub use_gpu: bool,
}

impl Default for DemucsConfig {
    fn default() -> Self {
        Self {
            model_path: "htdemucs".to_string(),
            segment_size: 10.0,
            overlap: 0.5,
            use_gpu: true,
        }
    }
}

/// Обновления прогресса Demucs
#[derive(Debug)]
pub enum DemucsProgress {
    Started,
    LoadingModel,
    Processing { progress: f32 },
    Converting,
    Finished,
    Error(String),
    Separating(f32),
    Completed,
}

/// Парсит прогресс из вывода Demucs
fn parse_demucs_progress(line: &str) -> Option<DemucsProgress> {
    if line.contains("Loading model") {
        Some(DemucsProgress::LoadingModel)
    } else if line.contains("Converting to mp3") {
        Some(DemucsProgress::Converting)
    } else if line.contains("progress") {
        // Пример строки: "progress: 45.5%"
        if let Some(percent) = line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.trim_end_matches('%').parse::<f32>().ok())
        {
            Some(DemucsProgress::Processing { progress: percent / 100.0 })
        } else {
            None
        }
    } else {
        None
    }
}

/// Проверяет, установлен ли Demucs
pub fn is_demucs_installed() -> bool {
    let output = Command::new("demucs")
        .arg("--help")
        .output();
    
    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Устанавливает Demucs
pub fn install_demucs() -> AppResult<()> {
    info!("Установка Demucs...");
    
    let status = Command::new("pip")
        .args(&["install", "demucs"])
        .status()
        .map_err(|e| AppError::InstallationError(format!("Ошибка установки Demucs: {}", e)))?;
        
    if !status.success() {
        return Err(AppError::InstallationError("Не удалось установить Demucs через pip".to_string()));
    }
    
    info!("Demucs успешно установлен");
    Ok(())
}

/// Проверяет, установлен ли Demucs, и устанавливает его при необходимости
pub fn ensure_demucs_installed() -> AppResult<()> {
    if !is_demucs_installed() {
        info!("Demucs не установлен, начинаем установку...");
        install_demucs()?;
    } else {
        info!("Demucs уже установлен");
    }
    Ok(())
}

/// Разделяет аудио на вокал и инструменты
pub async fn separate_audio(
    input_path: &Path,
    output_path: &Path,
    model: &str,
    device: &str,
    progress_sender: Option<Sender<DemucsProgress>>,
) -> AppResult<()> {
    info!("Separating audio using Demucs model {}", model);
    
    if let Some(sender) = &progress_sender {
        sender.send(DemucsProgress::Started).await?;
    }
    
    let mut cmd = TokioCommand::new("demucs");
    cmd.arg("--two-stems=vocals")
        .arg("--out")
        .arg(output_path)
        .arg("--device")
        .arg(device)
        .arg("--model")
        .arg(model)
        .arg(input_path);
        
    let mut child = cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
        
    if let Some(stderr) = child.stderr.take() {
        let mut reader = BufReader::new(stderr).lines();
        
        while let Ok(Some(line)) = reader.next_line().await {
            if let Some(progress) = parse_progress(&line) {
                if let Some(sender) = &progress_sender {
                    sender.send(DemucsProgress::Separating(progress)).await?;
                }
            }
        }
    }
    
    let status = child.wait().await?;
    if !status.success() {
        if let Some(sender) = &progress_sender {
            sender.send(DemucsProgress::Error("Demucs failed".to_string())).await?;
        }
        return Err("Demucs failed".into());
    }
    
    if let Some(sender) = &progress_sender {
        sender.send(DemucsProgress::Completed).await?;
    }
    
    Ok(())
}

fn parse_progress(line: &str) -> Option<f32> {
    if line.contains("Processed") && line.contains("%") {
        if let Some(percent) = line
            .split('%')
            .next()
            .and_then(|s| s.split_whitespace().last())
            .and_then(|s| s.parse::<f32>().ok())
        {
            return Some(percent / 100.0);
        }
    }
    None
}

/// Смешивает вокал и инструменты в заданной пропорции
pub fn mix_tracks(
    vocals_path: &Path,
    instrumental_path: &Path,
    output_path: &Path,
    voice_ratio: f32,
) -> AppResult<()> {
    ensure_ffmpeg_installed()?;

    // Рассчитываем коэффициенты для микширования
    let voice_volume = voice_ratio;
    let instrumental_volume = 1.0 - voice_ratio;

    // Создаем фильтр для смешивания аудио
    let filter = format!(
        "[0:a]volume={}[voice];[1:a]volume={}[instrumental];[voice][instrumental]amix=inputs=2:duration=longest",
        voice_volume,
        instrumental_volume
    );

    let status = Command::new("ffmpeg")
        .args(&[
            "-y",  // Перезаписывать выходной файл
            "-i", vocals_path.to_str().unwrap(),
            "-i", instrumental_path.to_str().unwrap(),
            "-filter_complex", &filter,
            "-ar", "44100",  // Частота дискретизации
            "-ac", "2",      // Количество каналов (стерео)
            "-c:a", "pcm_s16le",  // Кодек для WAV
            output_path.to_str().unwrap(),
        ])
        .status()
        .map_err(|e| AppError::AudioProcessingError(format!("Ошибка запуска ffmpeg: {}", e)))?;

    if !status.success() {
        return Err(AppError::AudioProcessingError("ffmpeg завершился с ошибкой при смешивании треков".to_string()));
    }

    Ok(())
} 