use std::path::Path;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::collections::HashMap;
use log::{info, warn, error};
use crate::errors::AppResult;
use crate::commands::speech_commands::Segment;

/// Разбирает VTT файл и возвращает сегменты
pub fn parse_vtt(path: &Path) -> AppResult<Vec<Segment>> {
    info!("Parsing VTT file: {}", path.display());
    
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut segments = Vec::new();
    let mut current_start = 0.0;
    let mut current_end = 0.0;
    let mut current_text = String::new();
    let mut parsing_content = false;
    
    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        
        // Пропускаем пустые строки и WEBVTT заголовок
        if trimmed.is_empty() || trimmed == "WEBVTT" {
            continue;
        }
        
        // Если строка имеет формат времени (00:00:00.000 --> 00:00:00.000)
        if trimmed.contains("-->") {
            // Если у нас уже есть текст, создаем сегмент
            if !current_text.is_empty() && current_end > current_start {
                segments.push(Segment {
                    text: current_text.trim().to_string(),
                    start: current_start,
                    end: current_end,
                });
                current_text = String::new();
            }
            
            // Парсим строку времени
            let parts: Vec<&str> = trimmed.split("-->").collect();
            if parts.len() == 2 {
                current_start = parse_timestamp(parts[0].trim()).unwrap_or(0.0);
                current_end = parse_timestamp(parts[1].trim()).unwrap_or(0.0);
            }
            
            parsing_content = true;
            continue;
        }
        
        // Если мы в режиме парсинга текста, добавляем строку
        if parsing_content && !trimmed.is_empty() {
            if !current_text.is_empty() {
                current_text.push_str(" ");
            }
            current_text.push_str(trimmed);
        }
    }
    
    // Добавляем последний сегмент, если он есть
    if !current_text.is_empty() && current_end > current_start {
        segments.push(Segment {
            text: current_text.trim().to_string(),
            start: current_start,
            end: current_end,
        });
    }
    
    info!("Parsed {} segments from VTT file", segments.len());
    Ok(segments)
}

/// Преобразует временную метку в секунды
fn parse_timestamp(timestamp: &str) -> Result<f64, String> {
    let parts: Vec<&str> = timestamp.split(':').collect();
    if parts.len() < 2 {
        return Err(format!("Invalid timestamp format: {}", timestamp));
    }
    
    let mut seconds = 0.0;
    
    // Парсим часы, если они есть
    if parts.len() == 3 {
        let hours = parts[0].parse::<f64>().map_err(|e| format!("Failed to parse hours: {}", e))?;
        seconds += hours * 3600.0;
    }
    
    // Парсим минуты
    let minutes_idx = if parts.len() == 3 { 1 } else { 0 };
    let minutes = parts[minutes_idx].parse::<f64>().map_err(|e| format!("Failed to parse minutes: {}", e))?;
    seconds += minutes * 60.0;
    
    // Парсим секунды и миллисекунды
    let seconds_idx = if parts.len() == 3 { 2 } else { 1 };
    let seconds_parts: Vec<&str> = parts[seconds_idx].split('.').collect();
    let secs = seconds_parts[0].parse::<f64>().map_err(|e| format!("Failed to parse seconds: {}", e))?;
    seconds += secs;
    
    // Добавляем миллисекунды, если они есть
    if seconds_parts.len() > 1 {
        let millis = seconds_parts[1].parse::<f64>().map_err(|e| format!("Failed to parse milliseconds: {}", e))?;
        seconds += millis / 1000.0;
    }
    
    Ok(seconds)
} 