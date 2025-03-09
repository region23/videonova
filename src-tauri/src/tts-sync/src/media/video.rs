//! Модуль для работы с видео
//! 
//! Этот модуль содержит функции для обработки видеофайлов.

use std::path::Path;
use crate::error::{Result, TtsSyncError};

/// Извлечение аудио из видео
pub fn extract_audio(video_path: &str, output_path: &str) -> Result<()> {
    let args = vec![
        "-i", video_path,
        "-vn", // Отключаем видео
        "-acodec", "libmp3lame", // Используем MP3 кодек
        "-q:a", "2", // Качество аудио
        "-y", output_path
    ];
    
    run_ffmpeg_command(&args)
}

/// Извлечение субтитров из видео
pub fn extract_subtitles(video_path: &str, output_path: &str) -> Result<()> {
    let args = vec![
        "-i", video_path,
        "-map", "0:s:0", // Выбираем первую дорожку субтитров
        "-y", output_path
    ];
    
    run_ffmpeg_command(&args)
}

/// Получение информации о видео
pub fn get_video_info(video_path: &str) -> Result<VideoInfo> {
    // Получаем длительность видео
    let duration = get_video_duration(video_path)?;
    
    // Получаем разрешение видео
    let (width, height) = get_video_resolution(video_path)?;
    
    // Получаем частоту кадров
    let fps = get_video_fps(video_path)?;
    
    Ok(VideoInfo {
        duration,
        width,
        height,
        fps,
    })
}

/// Информация о видео
#[derive(Debug, Clone)]
pub struct VideoInfo {
    /// Длительность видео в секундах
    pub duration: f64,
    /// Ширина видео в пикселях
    pub width: u32,
    /// Высота видео в пикселях
    pub height: u32,
    /// Частота кадров
    pub fps: f64,
}

/// Получение длительности видео
fn get_video_duration(video_path: &str) -> Result<f64> {
    let output = std::process::Command::new("ffprobe")
        .args(&[
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            video_path
        ])
        .output()?;
    
    if !output.status.success() {
        return Err(TtsSyncError::VideoProcessing(
            format!("FFprobe command failed with status: {}", output.status)
        ));
    }
    
    let duration_str = String::from_utf8_lossy(&output.stdout);
    let duration = duration_str.trim().parse::<f64>()
        .map_err(|_| TtsSyncError::VideoProcessing(
            format!("Failed to parse video duration: {}", duration_str)
        ))?;
    
    Ok(duration)
}

/// Получение разрешения видео
fn get_video_resolution(video_path: &str) -> Result<(u32, u32)> {
    let output = std::process::Command::new("ffprobe")
        .args(&[
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height",
            "-of", "csv=s=x:p=0",
            video_path
        ])
        .output()?;
    
    if !output.status.success() {
        return Err(TtsSyncError::VideoProcessing(
            format!("FFprobe command failed with status: {}", output.status)
        ));
    }
    
    let resolution_str = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = resolution_str.trim().split('x').collect();
    
    if parts.len() != 2 {
        return Err(TtsSyncError::VideoProcessing(
            format!("Failed to parse video resolution: {}", resolution_str)
        ));
    }
    
    let width = parts[0].parse::<u32>()
        .map_err(|_| TtsSyncError::VideoProcessing(
            format!("Failed to parse video width: {}", parts[0])
        ))?;
    
    let height = parts[1].parse::<u32>()
        .map_err(|_| TtsSyncError::VideoProcessing(
            format!("Failed to parse video height: {}", parts[1])
        ))?;
    
    Ok((width, height))
}

/// Получение частоты кадров видео
fn get_video_fps(video_path: &str) -> Result<f64> {
    let output = std::process::Command::new("ffprobe")
        .args(&[
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=r_frame_rate",
            "-of", "default=noprint_wrappers=1:nokey=1",
            video_path
        ])
        .output()?;
    
    if !output.status.success() {
        return Err(TtsSyncError::VideoProcessing(
            format!("FFprobe command failed with status: {}", output.status)
        ));
    }
    
    let fps_str = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = fps_str.trim().split('/').collect();
    
    if parts.len() != 2 {
        return Err(TtsSyncError::VideoProcessing(
            format!("Failed to parse video fps: {}", fps_str)
        ));
    }
    
    let numerator = parts[0].parse::<f64>()
        .map_err(|_| TtsSyncError::VideoProcessing(
            format!("Failed to parse fps numerator: {}", parts[0])
        ))?;
    
    let denominator = parts[1].parse::<f64>()
        .map_err(|_| TtsSyncError::VideoProcessing(
            format!("Failed to parse fps denominator: {}", parts[1])
        ))?;
    
    Ok(numerator / denominator)
}

/// Запуск команды FFmpeg
fn run_ffmpeg_command(args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("ffmpeg")
        .args(args)
        .status()?;
    
    if !status.success() {
        return Err(TtsSyncError::VideoProcessing(
            format!("FFmpeg command failed with status: {}", status)
        ));
    }
    
    Ok(())
}
