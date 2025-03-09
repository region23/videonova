//! Модуль для анализа временных меток субтитров
//! 
//! Этот модуль содержит функции для анализа временных меток субтитров.

use crate::subtitle::parser::Subtitle;

/// Метрики временных меток субтитров
#[derive(Debug, Clone)]
pub struct TimingMetrics {
    /// Средняя длительность субтитра
    pub avg_duration: f64,
    /// Минимальная длительность субтитра
    pub min_duration: f64,
    /// Максимальная длительность субтитра
    pub max_duration: f64,
    /// Средний интервал между субтитрами
    pub avg_gap: f64,
}

/// Анализ временных меток субтитров
pub fn analyze_subtitle_timing(subtitles: &[Subtitle]) -> TimingMetrics {
    if subtitles.is_empty() {
        return TimingMetrics {
            avg_duration: 0.0,
            min_duration: 0.0,
            max_duration: 0.0,
            avg_gap: 0.0,
        };
    }
    
    let mut total_duration: f64 = 0.0;
    let mut min_duration: f64 = f64::MAX;
    let mut max_duration: f64 = 0.0;
    let mut total_gap: f64 = 0.0;
    let mut gap_count = 0;
    
    for (i, subtitle) in subtitles.iter().enumerate() {
        let duration = subtitle.end_time.as_secs_f64() - subtitle.start_time.as_secs_f64();
        
        total_duration += duration;
        min_duration = min_duration.min(duration);
        max_duration = max_duration.max(duration);
        
        // Вычисляем интервал между текущим и следующим субтитром
        if i < subtitles.len() - 1 {
            let gap = subtitles[i + 1].start_time.as_secs_f64() - subtitle.end_time.as_secs_f64();
            if gap > 0.0 {
                total_gap += gap;
                gap_count += 1;
            }
        }
    }
    
    let avg_duration = total_duration / subtitles.len() as f64;
    let avg_gap = if gap_count > 0 { total_gap / gap_count as f64 } else { 0.0 };
    
    TimingMetrics {
        avg_duration,
        min_duration,
        max_duration,
        avg_gap,
    }
}
