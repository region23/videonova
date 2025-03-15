//! # VTT Parser
//! 
//! Модуль для разбора VTT-субтитров и их анализа.
//! Предоставляет функции для преобразования субтитров в формат,
//! удобный для генерации речи и оптимизации распределения времени.

use std::fs;
use std::path::Path;
use log::{info, warn};

use crate::utils::tts::types::{TtsError, Result, SubtitleCue, SegmentAnalysis, SegmentAnalysisConfig};

/// Конвертирует время из "00:00:00.000" формата в секунды.
fn vtt_time_to_seconds(time_str: &str) -> Result<f32> {
    let parts: Vec<&str> = time_str.trim().split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return Err(TtsError::VttParsingError(format!("Некорректный формат времени VTT: {}", time_str)));
    }
    
    let (hours, minutes, seconds) = if parts.len() == 3 {
        let h = parts[0].parse::<f32>().map_err(|_| {
            TtsError::VttParsingError(format!("Некорректный формат часов: {}", parts[0]))
        })?;
        let m = parts[1].parse::<f32>().map_err(|_| {
            TtsError::VttParsingError(format!("Некорректный формат минут: {}", parts[1]))
        })?;
        let s = parts[2].parse::<f32>().map_err(|_| {
            TtsError::VttParsingError(format!("Некорректный формат секунд: {}", parts[2]))
        })?;
        (h, m, s)
    } else {
        // Формат MM:SS.mmm
        let m = parts[0].parse::<f32>().map_err(|_| {
            TtsError::VttParsingError(format!("Некорректный формат минут: {}", parts[0]))
        })?;
        let s = parts[1].parse::<f32>().map_err(|_| {
            TtsError::VttParsingError(format!("Некорректный формат секунд: {}", parts[1]))
        })?;
        (0.0, m, s)
    };
    
    Ok(hours * 3600.0 + minutes * 60.0 + seconds)
}

/// Парсит VTT файл и возвращает список реплик.
/// 
/// # Аргументы
/// 
/// * `vtt_path` - Путь к VTT файлу
/// 
/// # Возвращает
/// 
/// Вектор реплик (SubtitleCue) или ошибку, если парсинг не удался
pub fn parse_vtt(vtt_path: &str) -> Result<Vec<SubtitleCue>> {
    let vtt_file = Path::new(vtt_path);
    if !vtt_file.exists() {
        return Err(TtsError::VttParsingError(format!("VTT файл не найден: {}", vtt_path)));
    }
    
    let contents = fs::read_to_string(vtt_file)
        .map_err(|e| TtsError::VttParsingError(format!("Не удалось прочитать VTT файл: {}", e)))?;
    
    let lines: Vec<&str> = contents.lines().collect();
    let mut cues = Vec::new();
    let mut i = 0;
    
    // Пропускаем заголовок WEBVTT и пустые строки в начале
    while i < lines.len() && (!lines[i].starts_with("WEBVTT") && i < 10 || lines[i].trim().is_empty()) {
        i += 1;
    }
    
    // Парсим кью
    while i < lines.len() {
        // Пропускаем пустые строки
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }
        
        // Пропускаем ID кью (если есть)
        if !lines[i].contains("-->") {
            i += 1;
            if i >= lines.len() {
                break;
            }
        }
        
        // Парсим временной интервал
        if !lines[i].contains("-->") {
            // Если не нашли интервал, переходим к следующей строке
            i += 1;
            continue;
        }
        
        let time_parts: Vec<&str> = lines[i].split("-->").collect();
        if time_parts.len() != 2 {
            i += 1;
            continue;
        }
        
        let start_time = vtt_time_to_seconds(time_parts[0])?;
        let end_time = vtt_time_to_seconds(time_parts[1])?;
        
        // Собираем текст субтитра
        i += 1;
        let mut text = String::new();
        while i < lines.len() && !lines[i].trim().is_empty() && !lines[i].contains("-->") {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(lines[i].trim());
            i += 1;
        }
        
        // Добавляем реплику в список
        if !text.is_empty() {
            cues.push(SubtitleCue {
                start: start_time,
                end: end_time,
                text,
            });
        }
    }
    
    info!("Прочитано {} реплик из VTT файла", cues.len());
    Ok(cues)
}

/// Анализирует субтитры на предмет проблемных сегментов,
/// где слишком много текста для заданного интервала времени.
/// 
/// # Аргументы
/// 
/// * `cues` - Список реплик для анализа
/// * `config` - Конфигурация для анализа
/// 
/// # Возвращает
/// 
/// Вектор результатов анализа сегментов
pub fn analyze_segments(cues: &[SubtitleCue], config: &SegmentAnalysisConfig) -> Vec<SegmentAnalysis> {
    let mut results = Vec::new();
    
    for (i, cue) in cues.iter().enumerate() {
        let word_count = cue.text.split_whitespace().count();
        let duration = cue.end - cue.start;
        
        // Для очень коротких сегментов проставляем ноль
        if duration < 0.1 {
            warn!("Очень короткий сегмент #{}: {:.3}s", i, duration);
            results.push(SegmentAnalysis {
                index: i,
                word_count,
                duration,
                words_per_second: 0.0,
                required_speed_factor: 0.0,
                severity: 10, // Максимальная критичность
            });
            continue;
        }
        
        let words_per_second = word_count as f32 / duration;
        let required_speed_factor = words_per_second / config.target_words_per_second;
        
        // Определяем критичность проблемы
        let severity = if words_per_second > config.max_words_per_second {
            // Находим значение между 0 и 10 на основе превышения допустимой скорости
            let excess_ratio = (words_per_second - config.max_words_per_second) / 
                                (config.max_words_per_second * 2.0); // предел = 3*max_words
            let severity_value = (excess_ratio * 10.0).min(10.0);
            severity_value.round() as u8
        } else {
            0
        };
        
        results.push(SegmentAnalysis {
            index: i,
            word_count,
            duration,
            words_per_second,
            required_speed_factor,
            severity,
        });
    }
    
    results
}

/// Оптимизирует распределение времени между сегментами субтитров,
/// чтобы улучшить качество озвучивания. "Заимствует" время у сегментов
/// с избытком времени и добавляет его сегментам с недостатком времени.
/// 
/// # Аргументы
/// 
/// * `cues` - Список реплик для оптимизации
/// * `critical_segments` - Список критичных сегментов, требующих больше времени
/// * `free_time_map` - Карта свободного времени для каждого сегмента
/// 
/// # Возвращает
/// 
/// Оптимизированный список реплик
pub fn optimize_time_distribution(
    mut cues: Vec<SubtitleCue>,
    analysis: &[SegmentAnalysis],
) -> Vec<SubtitleCue> {
    // Вычисляем, какие сегменты критичные (высокая плотность слов)
    let critical_segments: Vec<_> = analysis.iter()
        .filter(|s| s.severity > 7)
        .collect();
    
    // Если нет критичных сегментов, возвращаем оригинальные реплики
    if critical_segments.is_empty() {
        return cues;
    }
    
    info!("Перераспределение времени для {} критичных сегментов", critical_segments.len());
    
    // Создаем карту свободного времени для каждого сегмента
    let mut free_time_map: Vec<f32> = vec![0.0; cues.len()];
    
    // Для каждого сегмента определяем свободное время
    for i in 0..cues.len() {
        let word_count = cues[i].text.split_whitespace().count();
        let duration = cues[i].end - cues[i].start;
        
        // Предполагаем, что для комфортной речи нужно ~2.5 слова/сек
        let needed_time = word_count as f32 / 2.5;
        
        // Если у сегмента больше времени, чем нужно, отмечаем излишек
        if duration > needed_time && duration > 1.0 {
            // Оставляем минимум 0.5 сек для очень коротких сегментов и 80% длительности для длинных
            let min_duration = 0.5f32.max(needed_time * 1.2);
            let available = (duration - min_duration).max(0.0);
            
            free_time_map[i] = available;
        }
    }
    
    // Теперь для каждого критичного сегмента пробуем найти дополнительное время
    for segment in critical_segments {
        let idx = segment.index;
        let needed_duration = segment.word_count as f32 / 2.5;
        let current_duration = cues[idx].end - cues[idx].start;
        
        // Сколько дополнительного времени нам нужно для приемлемого ускорения (до 1.8x)
        let extra_time_needed = (needed_duration / 1.8 - current_duration).max(0.0);
        
        if extra_time_needed > 0.0 {
            let borrowed_time = 0.0;
            
            // Проверяем следующий сегмент, если он существует
            if idx + 1 < cues.len() && free_time_map[idx + 1] > 0.0 {
                let borrow_amount = free_time_map[idx + 1].min(extra_time_needed * 0.7);
                if borrow_amount > 0.0 {
                    // Заимствуем время, сдвигая конец текущего сегмента
                    cues[idx].end += borrow_amount;
                    
                    // Также сдвигаем начало следующего сегмента
                    cues[idx + 1].start += borrow_amount;
                    
                    // Обновляем карту свободного времени
                    free_time_map[idx + 1] -= borrow_amount;
                    
                    info!("Сегмент #{}: заимствовано {:.2}s у следующего сегмента", idx, borrow_amount);
                }
            }
            
            // Проверяем предыдущий сегмент, если он существует и нам все еще нужно время
            if borrowed_time < extra_time_needed && idx > 0 && free_time_map[idx - 1] > 0.0 {
                let remaining_need = extra_time_needed - borrowed_time;
                let borrow_amount = free_time_map[idx - 1].min(remaining_need * 0.7);
                
                if borrow_amount > 0.0 {
                    // Заимствуем время, сдвигая начало текущего сегмента
                    cues[idx].start -= borrow_amount;
                    
                    // Также сдвигаем конец предыдущего сегмента
                    cues[idx - 1].end -= borrow_amount;
                    
                    // Обновляем карту свободного времени
                    free_time_map[idx - 1] -= borrow_amount;
                    
                    info!("Сегмент #{}: заимствовано {:.2}s у предыдущего сегмента", idx, borrow_amount);
                }
            }
            
            // Обновляем длительность сегмента и записываем в лог
            let new_duration = cues[idx].end - cues[idx].start;
            let new_speed_factor = segment.word_count as f32 / 2.5 / new_duration;
            
            info!("Сегмент #{}: длительность {:.2}s -> {:.2}s, ускорение {:.2}x -> {:.2}x", 
                 idx, current_duration, new_duration, segment.required_speed_factor, new_speed_factor);
        }
    }
    
    cues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vtt_time_to_seconds() {
        // Тестируем формат MM:SS.mmm
        assert_eq!(vtt_time_to_seconds("01:30.500").unwrap(), 90.5);
        
        // Тестируем формат HH:MM:SS.mmm
        assert_eq!(vtt_time_to_seconds("01:01:30.500").unwrap(), 3690.5);
        
        // Тестируем некорректный формат
        assert!(vtt_time_to_seconds("invalid").is_err());
    }

    #[test]
    fn test_analyze_segments() {
        let cues = vec![
            SubtitleCue {
                start: 0.0,
                end: 2.0,
                text: "Это короткая фраза".to_string(),
            },
            SubtitleCue {
                start: 2.0,
                end: 3.0,
                text: "Это очень длинная фраза, которая должна быть произнесена за короткое время".to_string(),
            },
        ];
        
        let config = SegmentAnalysisConfig::default();
        let analysis = analyze_segments(&cues, &config);
        
        assert_eq!(analysis.len(), 2);
        assert_eq!(analysis[0].severity, 0); // Первая фраза нормальная
        assert!(analysis[1].severity > 5);   // Вторая фраза проблемная
    }
} 