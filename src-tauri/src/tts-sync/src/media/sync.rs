//! Модуль для синхронизации аудио и видео
//! 
//! Этот модуль содержит функции для синхронизации аудио с видео.

use std::path::Path;
use crate::error::{Result, TtsSyncError};
use crate::config::TtsSyncConfig;

/// Создание видео с несколькими аудиодорожками
pub fn create_multi_track_video(
    video_path: &str,
    original_audio_path: &str,
    tts_audio_path: &str,
    subtitles_path: &str,
    output_path: &str,
    config: &TtsSyncConfig,
) -> Result<String> {
    // Создаем временную директорию для промежуточных файлов
    let temp_dir = tempfile::tempdir()?;
    
    // Нормализуем громкость оригинального аудио
    let original_audio_normalized = temp_dir.path().join("original_normalized.mp3").to_string_lossy().to_string();
    crate::media::audio::normalize_audio_volume(
        original_audio_path,
        config.original_audio_volume,
        &original_audio_normalized,
    )?;
    
    // Нормализуем громкость TTS аудио
    let tts_audio_normalized = temp_dir.path().join("tts_normalized.mp3").to_string_lossy().to_string();
    crate::media::audio::normalize_audio_volume(
        tts_audio_path,
        config.tts_audio_volume,
        &tts_audio_normalized,
    )?;
    
    // Смешиваем аудиодорожки
    let mixed_audio = temp_dir.path().join("mixed_audio.mp3").to_string_lossy().to_string();
    crate::media::audio::mix_audio_files(
        &original_audio_normalized,
        1.0,
        &tts_audio_normalized,
        1.0,
        &mixed_audio,
    )?;
    
    // Создаем итоговое видео с смешанным аудио и субтитрами
    create_video_with_audio_and_subtitles(
        video_path,
        &mixed_audio,
        subtitles_path,
        output_path,
    )?;
    
    Ok(output_path.to_string())
}

/// Создание видео с аудио и субтитрами
fn create_video_with_audio_and_subtitles(
    video_path: &str,
    audio_path: &str,
    subtitles_path: &str,
    output_path: &str,
) -> Result<()> {
    let args = vec![
        "-i", video_path,
        "-i", audio_path,
        "-i", subtitles_path,
        "-map", "0:v", // Видео из первого входного файла
        "-map", "1:a", // Аудио из второго входного файла
        "-map", "2", // Субтитры из третьего входного файла
        "-c:v", "copy", // Копируем видео без перекодирования
        "-c:a", "aac", // Кодируем аудио в AAC
        "-c:s", "mov_text", // Кодируем субтитры в формат MOV
        "-metadata:s:s:0", "language=rus", // Устанавливаем язык субтитров
        "-y", output_path
    ];
    
    run_ffmpeg_command(&args)
}

/// Запуск команды FFmpeg
fn run_ffmpeg_command(args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("ffmpeg")
        .args(args)
        .status()?;
    
    if !status.success() {
        return Err(TtsSyncError::Synchronization(
            format!("FFmpeg command failed with status: {}", status)
        ));
    }
    
    Ok(())
}
