//! # Segment Analysis for TTS
//! 
//! Модуль для анализа сегментов субтитров, включая оценку скорости речи
//! и выявление потенциально проблемных фрагментов для синтеза речи.

use crate::utils::tts::types::SubtitleCue;

/// Конфигурация для анализа сегментов субтитров
#[derive(Debug, Clone)]
pub struct SegmentAnalysisConfig {
    /// Максимальное количество слов в секунду для комфортной речи
    pub max_words_per_second: f32,
    /// Максимальный коэффициент ускорения для разборчивой речи
    pub max_speed_factor: f32,
}

impl Default for SegmentAnalysisConfig {
    fn default() -> Self {
        Self {
            max_words_per_second: 2.5,
            max_speed_factor: 1.8,
        }
    }
}

/// Результат анализа отдельного сегмента субтитров
#[derive(Debug)]
pub struct SegmentAnalysisResult {
    /// Индекс сегмента
    pub index: usize,
    /// Количество слов в сегменте
    pub word_count: usize,
    /// Длительность сегмента в секундах
    pub duration: f32,
    /// Слов в секунду
    pub words_per_second: f32,
    /// Критичность проблемы (0-10)
    pub severity: u8,
    /// Требуемый коэффициент ускорения
    pub required_speed_factor: f32,
}

/// Функция для анализа сегментов субтитров
pub fn analyze_segments(cues: &[SubtitleCue], config: &SegmentAnalysisConfig) -> Vec<SegmentAnalysisResult> {
    let mut results = Vec::with_capacity(cues.len());
    
    for (i, cue) in cues.iter().enumerate() {
        let duration = cue.end - cue.start;
        let word_count = cue.text.split_whitespace().count();
        
        // Избегаем деления на ноль
        let words_per_second = if duration > 0.0 { word_count as f32 / duration } else { 0.0 };
        
        // Вычисляем, насколько скорость превышает максимально комфортную
        let required_speed_factor = if words_per_second > 0.0 { 
            words_per_second / config.max_words_per_second 
        } else { 
            0.0 
        };
        
        // Определяем критичность проблемы от 0 до 10
        let severity = if required_speed_factor <= 1.0 {
            // Если скорость ниже максимальной комфортной, проблемы нет
            0
        } else {
            // Линейно масштабируем от 1 до 10 по превышению max_speed_factor
            let severity_factor = (required_speed_factor - 1.0) / (config.max_speed_factor - 1.0);
            (severity_factor * 10.0).min(10.0) as u8
        };
        
        results.push(SegmentAnalysisResult {
            index: i,
            word_count,
            duration,
            words_per_second,
            severity,
            required_speed_factor,
        });
    }
    
    results
} 