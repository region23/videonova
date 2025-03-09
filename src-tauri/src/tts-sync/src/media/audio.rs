//! Модуль для работы с аудио
//! 
//! Этот модуль содержит функции для обработки аудиофайлов.

use std::path::Path;
use crate::error::{Result, TtsSyncError};

/// Изменение темпа аудио без изменения высоты тона
pub fn adjust_audio_tempo(input_file: &str, tempo_factor: f64, output_file: &str) -> Result<()> {
    let tempo_str = format!("{:.2}", tempo_factor);
    let filter_str = format!("atempo={}", tempo_str);
    let args = vec![
        "-i", input_file,
        "-filter:a", &filter_str,
        "-y", output_file
    ];
    
    run_ffmpeg_command(&args)
}

/// Нормализация громкости аудио
pub fn normalize_audio_volume(input_file: &str, volume: f32, output_file: &str) -> Result<()> {
    let volume_str = format!("{:.2}", volume);
    let filter_str = format!("volume={}", volume_str);
    let args = vec![
        "-i", input_file,
        "-filter:a", &filter_str,
        "-y", output_file
    ];
    
    run_ffmpeg_command(&args)
}

/// Объединение аудиофайлов
pub fn concat_audio_files(input_files: &[&str], output_file: &str) -> Result<()> {
    // Создаем временный файл со списком входных файлов
    let temp_dir = tempfile::tempdir()?;
    let concat_list_path = temp_dir.path().join("concat_list.txt");
    let mut concat_list = std::fs::File::create(&concat_list_path)?;
    
    // Записываем список файлов
    for file in input_files {
        use std::io::Write;
        writeln!(concat_list, "file '{}'", file)?;
    }
    
    // Закрываем файл
    drop(concat_list);
    
    // Запускаем FFmpeg для объединения файлов
    let args = vec![
        "-f", "concat",
        "-safe", "0",
        "-i", concat_list_path.to_str().unwrap(),
        "-c", "copy",
        "-y", output_file
    ];
    
    run_ffmpeg_command(&args)
}

/// Получение длительности аудиофайла
pub fn get_audio_duration(file_path: &str) -> Result<f64> {
    let output = std::process::Command::new("ffprobe")
        .args(&[
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            file_path
        ])
        .output()?;
    
    if !output.status.success() {
        return Err(TtsSyncError::AudioProcessing(
            format!("FFprobe command failed with status: {}", output.status)
        ));
    }
    
    let duration_str = String::from_utf8_lossy(&output.stdout);
    let duration = duration_str.trim().parse::<f64>()
        .map_err(|_| TtsSyncError::AudioProcessing(
            format!("Failed to parse audio duration: {}", duration_str)
        ))?;
    
    Ok(duration)
}

/// Запуск команды FFmpeg
fn run_ffmpeg_command(args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("ffmpeg")
        .args(args)
        .status()?;
    
    if !status.success() {
        return Err(TtsSyncError::AudioProcessing(
            format!("FFmpeg command failed with status: {}", status)
        ));
    }
    
    Ok(())
}

/// Смешивание двух аудиофайлов с разной громкостью
pub fn mix_audio_files(
    input_file1: &str,
    volume1: f32,
    input_file2: &str,
    volume2: f32,
    output_file: &str
) -> Result<()> {
    let filter_str = format!(
        "[0:a]volume={:.2}[a1];[1:a]volume={:.2}[a2];[a1][a2]amix=inputs=2:duration=longest",
        volume1,
        volume2
    );
    let args = vec![
        "-i", input_file1,
        "-i", input_file2,
        "-filter_complex", &filter_str,
        "-y", output_file
    ];
    
    run_ffmpeg_command(&args)
}

/// Обрезка аудиофайла
pub fn trim_audio(input_file: &str, start_time: f64, end_time: f64, output_file: &str) -> Result<()> {
    let start_str = format!("{:.3}", start_time);
    let end_str = format!("{:.3}", end_time);
    let args = vec![
        "-i", input_file,
        "-ss", &start_str,
        "-to", &end_str,
        "-y", output_file
    ];
    
    run_ffmpeg_command(&args)
}
