//! Модуль для оптимизации субтитров для TTS
//! 
//! Этот модуль содержит функции для оптимизации субтитров перед генерацией речи.

use crate::subtitle::parser::Subtitle;
use crate::subtitle::analyzer::TimingMetrics;
use crate::config::TtsSyncConfig;
use crate::error::Result;

/// Сложность синхронизации
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncComplexity {
    /// Низкая сложность
    Low,
    /// Средняя сложность
    Medium,
    /// Высокая сложность
    High,
}

/// Стратегия синхронизации
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStrategy {
    /// Точное соответствие временным меткам
    ExactTiming,
    /// Адаптивное изменение темпа речи
    AdaptiveTempo,
    /// Разделение длинных субтитров
    SplitLongSubtitles,
    /// Объединение коротких субтитров
    MergeShortSubtitles,
}

/// Оптимизация субтитров для TTS
pub fn optimize_for_tts(
    translated_subtitles: &[Subtitle],
    original_subtitles: &[Subtitle],
    metrics: &TimingMetrics,
    config: &TtsSyncConfig,
) -> Result<Vec<Subtitle>> {
    // Оцениваем сложность синхронизации
    let complexity = estimate_synchronization_complexity(translated_subtitles, original_subtitles, metrics);
    
    // Определяем стратегию синхронизации
    let strategy = determine_sync_strategy(complexity, config);
    
    // Применяем выбранную стратегию
    let mut optimized_subtitles = Vec::new();
    
    for (i, translated) in translated_subtitles.iter().enumerate() {
        // Находим соответствующий оригинальный субтитр
        let original = if i < original_subtitles.len() {
            &original_subtitles[i]
        } else {
            // Если нет соответствующего оригинального субтитра, используем переведенный
            translated
        };
        
        // Применяем стратегию оптимизации
        match strategy {
            SyncStrategy::ExactTiming => {
                // Используем точные временные метки оригинального субтитра
                optimized_subtitles.push(Subtitle::new(
                    original.start_time,
                    original.end_time,
                    translated.text.clone(),
                ));
            },
            SyncStrategy::AdaptiveTempo => {
                // Используем временные метки оригинального субтитра, но позже будем адаптировать темп речи
                optimized_subtitles.push(Subtitle::new(
                    original.start_time,
                    original.end_time,
                    translated.text.clone(),
                ));
            },
            SyncStrategy::SplitLongSubtitles => {
                // Разделяем длинные субтитры, если они превышают определенную длину
                let original_duration = original.end_time.as_secs_f64() - original.start_time.as_secs_f64();
                let words: Vec<&str> = translated.text.split_whitespace().collect();
                
                if words.len() > 15 && original_duration > 5.0 {
                    // Разделяем на две части
                    let mid = words.len() / 2;
                    let first_part = words[..mid].join(" ");
                    let second_part = words[mid..].join(" ");
                    
                    let mid_time = original.start_time.as_secs_f64() + original_duration / 2.0;
                    let mid_duration = std::time::Duration::from_secs_f64(mid_time);
                    
                    optimized_subtitles.push(Subtitle::new(
                        original.start_time,
                        mid_duration,
                        first_part,
                    ));
                    
                    optimized_subtitles.push(Subtitle::new(
                        mid_duration,
                        original.end_time,
                        second_part,
                    ));
                } else {
                    // Используем субтитр без изменений
                    optimized_subtitles.push(Subtitle::new(
                        original.start_time,
                        original.end_time,
                        translated.text.clone(),
                    ));
                }
            },
            SyncStrategy::MergeShortSubtitles => {
                // Объединяем короткие субтитры, если они идут подряд и между ними маленький интервал
                if i > 0 && i < translated_subtitles.len() - 1 {
                    let prev_original = &original_subtitles[i - 1];
                    let current_original = original;
                    
                    let gap = current_original.start_time.as_secs_f64() - prev_original.end_time.as_secs_f64();
                    
                    if gap < 0.3 && optimized_subtitles.len() > 0 {
                        // Объединяем с предыдущим субтитром
                        let last_index = optimized_subtitles.len() - 1;
                        let combined_text = format!("{} {}", optimized_subtitles[last_index].text, translated.text);
                        
                        optimized_subtitles[last_index] = Subtitle::new(
                            optimized_subtitles[last_index].start_time,
                            current_original.end_time,
                            combined_text,
                        );
                    } else {
                        // Используем субтитр без изменений
                        optimized_subtitles.push(Subtitle::new(
                            current_original.start_time,
                            current_original.end_time,
                            translated.text.clone(),
                        ));
                    }
                } else {
                    // Используем субтитр без изменений
                    optimized_subtitles.push(Subtitle::new(
                        original.start_time,
                        original.end_time,
                        translated.text.clone(),
                    ));
                }
            },
        }
    }
    
    Ok(optimized_subtitles)
}

/// Оценка сложности синхронизации
fn estimate_synchronization_complexity(
    translated_subtitles: &[Subtitle],
    original_subtitles: &[Subtitle],
    metrics: &TimingMetrics,
) -> SyncComplexity {
    // Проверяем соответствие количества субтитров
    if translated_subtitles.len() != original_subtitles.len() {
        return SyncComplexity::High;
    }
    
    // Проверяем среднюю длину текста
    let avg_original_length: f64 = original_subtitles.iter()
        .map(|s| s.text.len() as f64)
        .sum::<f64>() / original_subtitles.len() as f64;
    
    let avg_translated_length: f64 = translated_subtitles.iter()
        .map(|s| s.text.len() as f64)
        .sum::<f64>() / translated_subtitles.len() as f64;
    
    let length_ratio = avg_translated_length / avg_original_length;
    
    if length_ratio > 1.5 || length_ratio < 0.5 {
        return SyncComplexity::High;
    } else if length_ratio > 1.2 || length_ratio < 0.8 {
        return SyncComplexity::Medium;
    }
    
    SyncComplexity::Low
}

/// Определение стратегии синхронизации
fn determine_sync_strategy(
    complexity: SyncComplexity,
    config: &TtsSyncConfig,
) -> SyncStrategy {
    match config.sync_method {
        crate::config::SyncMethod::Simple => SyncStrategy::ExactTiming,
        crate::config::SyncMethod::Adaptive => SyncStrategy::AdaptiveTempo,
        crate::config::SyncMethod::Auto => {
            // Автоматический выбор стратегии в зависимости от сложности
            match complexity {
                SyncComplexity::Low => SyncStrategy::ExactTiming,
                SyncComplexity::Medium => SyncStrategy::AdaptiveTempo,
                SyncComplexity::High => SyncStrategy::SplitLongSubtitles,
            }
        }
    }
}
