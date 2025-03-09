//! Модуль для работы с FFmpeg
//! 
//! Этот модуль содержит функции для работы с FFmpeg.

use std::process::Command;
use crate::error::{Result, TtsSyncError};

/// Проверка наличия FFmpeg
pub fn check_ffmpeg_installed() -> Result<bool> {
    let output = Command::new("ffmpeg")
        .arg("-version")
        .output()?;
    
    Ok(output.status.success())
}

/// Получение версии FFmpeg
pub fn get_ffmpeg_version() -> Result<String> {
    let output = Command::new("ffmpeg")
        .arg("-version")
        .output()?;
    
    if !output.status.success() {
        return Err(TtsSyncError::Other("Failed to get FFmpeg version".to_string()));
    }
    
    let version_str = String::from_utf8_lossy(&output.stdout);
    let first_line = version_str.lines().next().unwrap_or("");
    
    Ok(first_line.to_string())
}

/// Запуск команды FFmpeg
pub fn run_ffmpeg_command(args: &[&str]) -> Result<()> {
    let status = Command::new("ffmpeg")
        .args(args)
        .status()?;
    
    if !status.success() {
        return Err(TtsSyncError::Other(
            format!("FFmpeg command failed with status: {}", status)
        ));
    }
    
    Ok(())
}

/// Запуск команды FFprobe
pub fn run_ffprobe_command(args: &[&str]) -> Result<String> {
    let output = Command::new("ffprobe")
        .args(args)
        .output()?;
    
    if !output.status.success() {
        return Err(TtsSyncError::Other(
            format!("FFprobe command failed with status: {}", output.status)
        ));
    }
    
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
