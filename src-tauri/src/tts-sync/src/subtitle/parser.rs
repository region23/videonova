//! Модуль для парсинга субтитров
//! 
//! Этот модуль содержит функции для парсинга VTT файлов.

use std::path::Path;
use std::time::Duration;
use crate::error::Result;

/// Структура для хранения субтитра
#[derive(Debug, Clone)]
pub struct Subtitle {
    /// Время начала субтитра
    pub start_time: Duration,
    /// Время окончания субтитра
    pub end_time: Duration,
    /// Текст субтитра
    pub text: String,
}

impl Subtitle {
    /// Создать новый экземпляр Subtitle
    pub fn new(start_time: Duration, end_time: Duration, text: String) -> Self {
        Self {
            start_time,
            end_time,
            text,
        }
    }
}

/// Парсинг VTT файла
pub fn parse_vtt_file<P: AsRef<Path>>(vtt_file_path: P) -> Result<Vec<Subtitle>> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use crate::error::TtsSyncError;
    
    let file = File::open(&vtt_file_path)
        .map_err(|e| TtsSyncError::FileNotFound(format!("Failed to open VTT file: {}", e)))?;
    
    let reader = BufReader::new(file);
    let mut lines: Vec<String> = reader.lines()
        .filter_map(|line| line.ok())
        .collect();
    
    // Проверяем заголовок WebVTT
    if lines.is_empty() || !lines[0].contains("WEBVTT") {
        return Err(TtsSyncError::InvalidFormat("Invalid VTT file format: missing WEBVTT header".to_string()));
    }
    
    // Удаляем заголовок и пустые строки в начале
    lines.remove(0);
    while !lines.is_empty() && lines[0].trim().is_empty() {
        lines.remove(0);
    }
    
    let mut subtitles = Vec::new();
    let mut current_block = Vec::new();
    
    for line in lines {
        if line.trim().is_empty() {
            if !current_block.is_empty() {
                if let Some(subtitle) = parse_cue_block(&current_block) {
                    subtitles.push(subtitle);
                }
                current_block.clear();
            }
        } else {
            current_block.push(line);
        }
    }
    
    // Обрабатываем последний блок, если он есть
    if !current_block.is_empty() {
        if let Some(subtitle) = parse_cue_block(&current_block) {
            subtitles.push(subtitle);
        }
    }
    
    Ok(subtitles)
}

/// Парсинг блока субтитра
fn parse_cue_block(lines: &[String]) -> Option<Subtitle> {
    if lines.len() < 2 {
        return None;
    }
    
    // Ищем строку с временными метками
    let timing_line = lines.iter().find(|line| line.contains("-->"))?;
    let parts: Vec<&str> = timing_line.split("-->").collect();
    if parts.len() != 2 {
        return None;
    }
    
    let start_time = parse_time_str(parts[0].trim())?;
    let end_time = parse_time_str(parts[1].trim())?;
    
    // Собираем текст субтитра
    let text = lines.iter()
        .skip_while(|line| line.contains("-->"))
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<&str>>()
        .join(" ");
    
    Some(Subtitle::new(start_time, end_time, text))
}

/// Парсинг строки времени в формате HH:MM:SS.mmm
fn parse_time_str(time_str: &str) -> Option<Duration> {
    let parts: Vec<&str> = time_str.trim().split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    
    let (hours, minutes, seconds) = if parts.len() == 3 {
        let hours = parts[0].parse::<u64>().ok()?;
        let minutes = parts[1].parse::<u64>().ok()?;
        let seconds_parts: Vec<&str> = parts[2].split('.').collect();
        let seconds = seconds_parts[0].parse::<u64>().ok()?;
        let milliseconds = if seconds_parts.len() > 1 {
            let ms_str = seconds_parts[1];
            let ms = ms_str.parse::<u64>().ok()?;
            match ms_str.len() {
                1 => ms * 100,
                2 => ms * 10,
                3 => ms,
                _ => ms / 10_u64.pow(ms_str.len() as u32 - 3),
            }
        } else {
            0
        };
        (hours, minutes, seconds * 1000 + milliseconds)
    } else {
        let minutes = parts[0].parse::<u64>().ok()?;
        let seconds_parts: Vec<&str> = parts[1].split('.').collect();
        let seconds = seconds_parts[0].parse::<u64>().ok()?;
        let milliseconds = if seconds_parts.len() > 1 {
            let ms_str = seconds_parts[1];
            let ms = ms_str.parse::<u64>().ok()?;
            match ms_str.len() {
                1 => ms * 100,
                2 => ms * 10,
                3 => ms,
                _ => ms / 10_u64.pow(ms_str.len() as u32 - 3),
            }
        } else {
            0
        };
        (0, minutes, seconds * 1000 + milliseconds)
    };
    
    Some(Duration::from_millis(hours * 3600000 + minutes * 60000 + seconds))
}
