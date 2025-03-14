use super::types::{AudioFragment, FragmentProcessingConfig};
use super::processor::{process_fragment, apply_fade_in, apply_fade_out};
use crate::errors::{AppError, AppResult};
use anyhow::anyhow;

/// Склеивает аудио фрагменты в один
pub fn merge_fragments(
    fragments: &mut [AudioFragment],
    config: &FragmentProcessingConfig,
) -> AppResult<Vec<f32>> {
    if fragments.is_empty() {
        return Ok(Vec::new());
    }

    // Проверяем, что все фрагменты имеют одинаковую частоту дискретизации
    let sample_rate = fragments[0].sample_rate;
    if !fragments.iter().all(|f| f.sample_rate == sample_rate) {
        return Err(anyhow!("Все фрагменты должны иметь одинаковую частоту дискретизации").into());
    }

    // Обрабатываем каждый фрагмент
    for fragment in fragments.iter_mut() {
        process_fragment(fragment, config)?;
    }

    // Вычисляем общую длительность
    let total_duration = fragments
        .iter()
        .map(|f| f.samples_duration())
        .sum::<usize>();

    // Создаем буфер для результата
    let mut result = vec![0.0; total_duration];
    let mut current_position = 0;

    // Склеиваем фрагменты
    for fragment in fragments {
        let fragment_len = fragment.samples_duration();
        
        // Копируем сэмплы
        result[current_position..current_position + fragment_len]
            .copy_from_slice(&fragment.samples);
        
        current_position += fragment_len;
    }

    Ok(result)
}

/// Склеивает аудио фрагменты с кроссфейдом
pub fn merge_fragments_with_crossfade(
    fragments: &mut [AudioFragment],
    config: &FragmentProcessingConfig,
    crossfade_duration: f32,
) -> AppResult<Vec<f32>> {
    if fragments.is_empty() {
        return Ok(Vec::new());
    }

    // Проверяем, что все фрагменты имеют одинаковую частоту дискретизации
    let sample_rate = fragments[0].sample_rate;
    if !fragments.iter().all(|f| f.sample_rate == sample_rate) {
        return Err(anyhow!("Все фрагменты должны иметь одинаковую частоту дискретизации").into());
    }

    // Обрабатываем каждый фрагмент
    for fragment in fragments.iter_mut() {
        process_fragment(fragment, config)?;
    }

    // Вычисляем длительность кроссфейда в сэмплах
    let crossfade_samples = (crossfade_duration * sample_rate as f32) as usize;

    // Вычисляем общую длительность с учетом кроссфейда
    let total_duration = fragments
        .iter()
        .map(|f| f.samples_duration())
        .sum::<usize>()
        - (fragments.len() - 1) * crossfade_samples;

    // Создаем буфер для результата
    let mut result = vec![0.0; total_duration];
    let mut current_position = 0;

    // Склеиваем фрагменты с кроссфейдом
    for (i, fragment) in fragments.iter().enumerate() {
        let fragment_len = fragment.samples_duration();
        
        if i > 0 {
            // Применяем кроссфейд с предыдущим фрагментом
            let fade_start = current_position;
            let fade_end = fade_start + crossfade_samples;
            
            // Накладываем фрагменты друг на друга
            for j in 0..crossfade_samples {
                let fade_out = 0.5 * (1.0 + ((std::f32::consts::PI * j as f32) / crossfade_samples as f32).cos());
                let fade_in = 1.0 - fade_out;
                
                result[fade_start + j] = result[fade_start + j] * fade_out + fragment.samples[j] * fade_in;
            }
            
            // Копируем оставшуюся часть фрагмента
            result[fade_end..fade_end + fragment_len - crossfade_samples]
                .copy_from_slice(&fragment.samples[crossfade_samples..]);
            
            current_position = fade_end + fragment_len - crossfade_samples;
        } else {
            // Первый фрагмент копируем полностью
            result[..fragment_len].copy_from_slice(&fragment.samples);
            current_position = fragment_len;
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_merge_fragments() {
        let mut fragments = vec![
            AudioFragment::new(
                0,
                Duration::from_secs(0),
                Duration::from_secs(1),
                vec![1.0; 44100],
                44100,
            ),
            AudioFragment::new(
                1,
                Duration::from_secs(1),
                Duration::from_secs(2),
                vec![1.0; 44100],
                44100,
            ),
        ];

        let config = FragmentProcessingConfig::default();
        let result = merge_fragments(&mut fragments, &config).unwrap();

        assert_eq!(result.len(), 88200); // 2 секунды при 44100 Hz
    }

    #[test]
    fn test_merge_fragments_with_crossfade() {
        let mut fragments = vec![
            AudioFragment::new(
                0,
                Duration::from_secs(0),
                Duration::from_secs(1),
                vec![1.0; 44100],
                44100,
            ),
            AudioFragment::new(
                1,
                Duration::from_secs(1),
                Duration::from_secs(2),
                vec![1.0; 44100],
                44100,
            ),
        ];

        let config = FragmentProcessingConfig::default();
        let result = merge_fragments_with_crossfade(&mut fragments, &config, 0.1).unwrap();

        // Длина должна быть меньше на длительность кроссфейда
        assert_eq!(result.len(), 88200 - 4410); // 2 секунды - 0.1 секунда кроссфейда при 44100 Hz
    }
} 