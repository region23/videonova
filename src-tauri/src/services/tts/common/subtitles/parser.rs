use std::path::Path;
use std::time::Duration;
use std::fs::File;
use std::io::{BufRead, BufReader};
use super::types::{SubtitleCue, VttParseResult, VttError};

/// Парсит время из строки формата "HH:MM:SS.mmm"
fn parse_timestamp(timestamp: &str) -> Result<Duration, VttError> {
    let parts: Vec<&str> = timestamp.split(':').collect();
    if parts.len() != 3 {
        return Err(VttError::TimeParseError(format!(
            "Неверный формат времени: {}",
            timestamp
        )));
    }

    let hours: u64 = parts[0].parse().map_err(|_| {
        VttError::TimeParseError(format!("Неверный формат часов: {}", parts[0]))
    })?;

    let minutes: u64 = parts[1].parse().map_err(|_| {
        VttError::TimeParseError(format!("Неверный формат минут: {}", parts[1]))
    })?;

    let seconds_parts: Vec<&str> = parts[2].split('.').collect();
    if seconds_parts.len() != 2 {
        return Err(VttError::TimeParseError(format!(
            "Неверный формат секунд: {}",
            parts[2]
        )));
    }

    let seconds: u64 = seconds_parts[0].parse().map_err(|_| {
        VttError::TimeParseError(format!("Неверный формат секунд: {}", seconds_parts[0]))
    })?;

    let milliseconds: u64 = seconds_parts[1].parse().map_err(|_| {
        VttError::TimeParseError(format!(
            "Неверный формат миллисекунд: {}",
            seconds_parts[1]
        ))
    })?;

    Ok(Duration::from_millis(
        hours * 3600000 + minutes * 60000 + seconds * 1000 + milliseconds,
    ))
}

/// Парсит временной интервал из строки формата "HH:MM:SS.mmm --> HH:MM:SS.mmm"
fn parse_time_range(line: &str) -> Result<(Duration, Duration), VttError> {
    let parts: Vec<&str> = line.split("-->").collect();
    if parts.len() != 2 {
        return Err(VttError::InvalidFormat(format!(
            "Неверный формат временного интервала: {}",
            line
        )));
    }

    let start = parse_timestamp(parts[0].trim())?;
    let end = parse_timestamp(parts[1].trim())?;

    Ok((start, end))
}

/// Парсит VTT файл
pub fn parse_vtt<P: AsRef<Path>>(path: P) -> Result<VttParseResult, VttError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    // Пропускаем заголовок WEBVTT
    match lines.next() {
        Some(Ok(first_line)) if first_line.trim() == "WEBVTT" => {}
        _ => return Err(VttError::InvalidFormat("Отсутствует заголовок WEBVTT".to_string())),
    }

    let mut cues = Vec::new();
    let mut current_index = 0;
    let mut current_time_range: Option<(Duration, Duration)> = None;
    let mut current_text = String::new();

    for line in lines {
        let line = line?;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            // Пустая строка означает конец текущего субтитра
            if let Some((start, end)) = current_time_range.take() {
                if !current_text.is_empty() {
                    cues.push(SubtitleCue::new(
                        current_index,
                        start,
                        end,
                        current_text.trim().to_string(),
                    ));
                    current_index += 1;
                    current_text.clear();
                }
            }
        } else if trimmed.contains("-->") {
            // Строка с временным интервалом
            current_time_range = Some(parse_time_range(trimmed)?);
        } else if current_time_range.is_some() {
            // Строка с текстом
            if !current_text.is_empty() {
                current_text.push('\n');
            }
            current_text.push_str(trimmed);
        }
    }

    // Обрабатываем последний субтитр
    if let Some((start, end)) = current_time_range {
        if !current_text.is_empty() {
            cues.push(SubtitleCue::new(
                current_index,
                start,
                end,
                current_text.trim().to_string(),
            ));
        }
    }

    // Вычисляем общую длительность
    let duration = cues
        .last()
        .map(|cue| cue.end)
        .unwrap_or(Duration::from_secs(0));

    Ok(VttParseResult { cues, duration })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_timestamp() {
        let result = parse_timestamp("00:01:23.456").unwrap();
        assert_eq!(result, Duration::from_millis(83456));
    }

    #[test]
    fn test_parse_time_range() {
        let (start, end) = parse_time_range("00:01:23.456 --> 00:02:34.567").unwrap();
        assert_eq!(start, Duration::from_millis(83456));
        assert_eq!(end, Duration::from_millis(154567));
    }
} 