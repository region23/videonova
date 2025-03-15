//! # Audio Processing
//! 
//! Модуль для обработки аудиоданных, включая изменение длительности,
//! нормализацию, микширование и другие операции.

use std::cmp;
use log::{info, warn, error};
use rubato::{SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction, Resampler};

use crate::utils::tts::types::{TtsError, Result, AudioProcessingConfig};
use crate::utils::tts::soundtouch;
use crate::utils::tts::audio_format;

/// Корректирует длительность аудиофрагмента без обрезки, используя time-stretching.
///
/// # Аргументы
///
/// * `input` - Входные PCM-семплы (f32)
/// * `actual_duration` - Текущая длительность аудио в секундах
/// * `target_duration` - Целевая длительность в секундах
/// * `available_extra_time` - Доступное дополнительное время после этого фрагмента
/// * `sample_rate` - Частота дискретизации
/// * `config` - Конфигурация аудио-обработки
///
/// # Возвращает
///
/// Кортеж с обработанными PCM-семплами и фактически использованной длительностью
pub fn adjust_duration(
    input: &[f32], 
    actual_duration: f32, 
    target_duration: f32, 
    available_extra_time: f32,
    sample_rate: u32,
    config: &AudioProcessingConfig
) -> Result<(Vec<f32>, f32)> {
    let duration_ratio = target_duration / actual_duration;
    
    // Если соотношение близко к 1, ничего не меняем
    if (duration_ratio - 1.0).abs() < 0.02 {
        info!("Корректировка не требуется: соотношение длительностей {:.3}", duration_ratio);
        return Ok((input.to_vec(), target_duration));
    }
    
    // Если у нас есть дополнительное время, используем его для улучшения результата
    let adjusted_target = if duration_ratio < 1.0 && available_extra_time > 0.0 {
        // При ускорении, рассмотрим использование дополнительного времени
        let extra_usage = (1.0 - duration_ratio) * config.extra_time_usage_factor * actual_duration;
        let extra_time_to_use = extra_usage.min(available_extra_time);
        
        if extra_time_to_use > 0.01 {
            info!("Используем {:.3}s дополнительного времени (доступно {:.3}s)", 
                 extra_time_to_use, available_extra_time);
            target_duration + extra_time_to_use
        } else {
            target_duration
        }
    } else {
        target_duration
    };
    
    // Новое соотношение с учетом дополнительного времени
    let new_ratio = adjusted_target / actual_duration;
    
    // Если все равно нужно ускорение или замедление, применяем алгоритмы
    if new_ratio < config.min_rubato_speed {
        // Сильное ускорение - используем SoundTouch
        let safe_ratio = new_ratio.max(config.min_soundtouch_speed);
        info!("Сильное ускорение: используем SoundTouch с коэффициентом {:.3}", safe_ratio);
        
        // Используем SoundTouch для больших изменений темпа
        let tempo_factor = 1.0 / safe_ratio; // Инвертируем для SoundTouch (>1 = ускорение)
        match soundtouch::process_with_soundtouch(&input, sample_rate, tempo_factor) {
            Ok(stretched) => {
                let used_duration = adjusted_target;
                Ok((stretched, used_duration))
            },
            Err(e) => {
                error!("Ошибка SoundTouch: {}", e);
                // Падаем обратно на Rubato для надежности
                stretch_with_rubato(input, safe_ratio, sample_rate)
                    .map(|stretched| (stretched, adjusted_target))
            }
        }
    } else if new_ratio > config.max_rubato_speed {
        // Сильное замедление - используем SoundTouch
        let safe_ratio = new_ratio.min(config.max_soundtouch_speed);
        info!("Сильное замедление: используем SoundTouch с коэффициентом {:.3}", safe_ratio);
        
        // Используем SoundTouch для больших изменений темпа
        let tempo_factor = 1.0 / safe_ratio; // Инвертируем для SoundTouch (<1 = замедление)
        match soundtouch::process_with_soundtouch(&input, sample_rate, tempo_factor) {
            Ok(stretched) => {
                let used_duration = adjusted_target;
                Ok((stretched, used_duration))
            },
            Err(e) => {
                error!("Ошибка SoundTouch: {}", e);
                // Падаем обратно на Rubato для надежности
                stretch_with_rubato(input, safe_ratio, sample_rate)
                    .map(|stretched| (stretched, adjusted_target))
            }
        }
    } else {
        // Умеренные изменения темпа - используем Rubato
        info!("Умеренное изменение темпа: используем Rubato с коэффициентом {:.3}", new_ratio);
        stretch_with_rubato(input, new_ratio, sample_rate)
            .map(|stretched| (stretched, adjusted_target))
    }
}

/// Изменяет длительность аудио с помощью Rubato (высококачественный ресэмплер).
/// 
/// # Аргументы
/// 
/// * `input` - Входные PCM-семплы (f32)
/// * `ratio` - Соотношение целевой длительности к исходной
/// * `sample_rate` - Частота дискретизации
/// 
/// # Возвращает
/// 
/// Обработанные PCM-семплы
fn stretch_with_rubato(input: &[f32], ratio: f32, sample_rate: u32) -> Result<Vec<f32>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    
    // Определяем размер блока в зависимости от длительности
    let duration_seconds = input.len() as f32 / sample_rate as f32;
    let block_size = if duration_seconds < 0.1 {
        64 // Очень короткие фрагменты
    } else if duration_seconds < 0.5 {
        128 // Короткие фрагменты
    } else if duration_seconds < 2.0 {
        256 // Средние фрагменты
    } else {
        512 // Длинные фрагменты
    };
    
    // Параметры sinc-интерполяции для высокого качества
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    
    // Создаем ресэмплер
    let mut resampler = SincFixedIn::<f32>::new(
        ratio as f64,
        1.0,
        params,
        block_size,
        1 // моно
    ).map_err(|e| TtsError::TimeStretchingError(format!("Ошибка инициализации Rubato: {}", e)))?;
    
    // Подготавливаем входные данные
    let mut input_frames = vec![Vec::new()];
    input_frames[0].extend_from_slice(input);
    
    // Предварительно выделяем память для выходных данных
    let output_size = (input.len() as f32 * ratio) as usize;
    let mut output_buf = vec![0.0; output_size + block_size * 2]; // с запасом
    let mut total_output = 0;
    
    // Обработка блоками
    let mut idx = 0;
    while idx < input.len() {
        let chunk_size = cmp::min(block_size, input.len() - idx);
        if chunk_size == 0 {
            break;
        }
        
        // Если у нас последний блок и он слишком мал, используем padding
        let current_chunk = if chunk_size < block_size / 4 && idx > 0 {
            let mut padded = vec![0.0; block_size];
            padded[..chunk_size].copy_from_slice(&input[idx..idx+chunk_size]);
            padded
        } else {
            input[idx..idx+chunk_size].to_vec()
        };
        
        // Подготавливаем входные данные для текущего блока
        let mut current_frames = vec![current_chunk];
        
        // Обрабатываем блок
        let output_frames = resampler.process(&current_frames, None)
            .map_err(|e| TtsError::TimeStretchingError(format!("Ошибка в процессе ресемплинга: {}", e)))?;
        
        // Копируем результат
        let output_len = output_frames[0].len();
        if total_output + output_len <= output_buf.len() {
            output_buf[total_output..total_output+output_len].copy_from_slice(&output_frames[0]);
            total_output += output_len;
        } else {
            return Err(TtsError::TimeStretchingError(
                format!("Переполнение выходного буфера при ресемплинге: {} + {} > {}", 
                    total_output, output_len, output_buf.len()
                )
            ));
        }
        
        idx += chunk_size;
    }
    
    // Обрезаем выходной буфер до фактического размера
    output_buf.truncate(total_output);
    
    Ok(output_buf)
}

/// Применяет fade in/out к аудиофрагменту для устранения щелчков.
/// 
/// # Аргументы
/// 
/// * `samples` - PCM-семплы для обработки (изменяются на месте)
/// * `fade_ms` - Длительность fade in/out в миллисекундах
/// * `sample_rate` - Частота дискретизации
pub fn apply_fade(samples: &mut [f32], fade_ms: u32, sample_rate: u32) {
    if samples.is_empty() {
        return;
    }
    
    let fade_samples = ((fade_ms as f32 / 1000.0) * sample_rate as f32) as usize;
    if fade_samples * 2 >= samples.len() {
        // Если fade слишком длинный для текущего фрагмента, уменьшаем его
        let adjusted_fade = samples.len() / 4;
        
        // Fade in
        for i in 0..adjusted_fade {
            let factor = i as f32 / adjusted_fade as f32;
            samples[i] *= factor;
        }
        
        // Fade out
        for i in 0..adjusted_fade {
            let factor = 1.0 - (i as f32 / adjusted_fade as f32);
            let idx = samples.len() - 1 - i;
            samples[idx] *= factor;
        }
    } else {
        // Обычный случай: fade in в начале
        for i in 0..fade_samples {
            let factor = i as f32 / fade_samples as f32;
            samples[i] *= factor;
        }
        
        // Fade out в конце
        for i in 0..fade_samples {
            let factor = 1.0 - (i as f32 / fade_samples as f32);
            let idx = samples.len() - 1 - i;
            samples[idx] *= factor;
        }
    }
}

/// Микширует голосовую и инструментальную дорожки с указанными параметрами.
/// 
/// # Аргументы
/// 
/// * `voice_track` - Голосовая дорожка (PCM, f32)
/// * `instrumental_track` - Инструментальная дорожка (PCM, f32)
/// * `voice_to_instrumental_ratio` - Соотношение громкости голоса к инструменталу
/// * `instrumental_boost` - Дополнительное усиление инструментала
/// 
/// # Возвращает
/// 
/// Смикшированные PCM-семплы
pub fn mix_audio_tracks(
    voice_track: &[f32], 
    instrumental_track: &[f32], 
    voice_to_instrumental_ratio: f32,
    instrumental_boost: f32
) -> Vec<f32> {
    if voice_track.is_empty() {
        return instrumental_track.to_vec();
    }
    
    if instrumental_track.is_empty() {
        return voice_track.to_vec();
    }
    
    // Вычисляем коэффициенты для миксования
    let voice_rms = audio_format::compute_rms(voice_track);
    let instrumental_rms = audio_format::compute_rms(instrumental_track);
    
    if voice_rms < 0.00001 || instrumental_rms < 0.00001 {
        warn!("Очень низкий уровень одной из дорожек для микширования: голос={:.6}, инструментал={:.6}", 
              voice_rms, instrumental_rms);
        return voice_track.to_vec();
    }
    
    // Рассчитываем уровни для микширования на основе соотношения и усиления
    let voice_level = 1.0;
    let instrumental_level = (voice_rms / instrumental_rms) / voice_to_instrumental_ratio * instrumental_boost;
    
    info!("Микширование: уровень голоса={:.6}, уровень инструментала={:.6}", voice_level, instrumental_level);
    
    // Создаем микшированный трек
    let min_len = voice_track.len().min(instrumental_track.len());
    let mut mixed = Vec::with_capacity(min_len);
    
    // Микшируем до длины самого короткого трека
    for i in 0..min_len {
        let mixed_sample = voice_track[i] * voice_level + instrumental_track[i] * instrumental_level;
        mixed.push(mixed_sample);
    }
    
    // Нормализуем микс, если уровень слишком высокий
    let max_amplitude = mixed.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    if max_amplitude > 1.0 {
        let norm_factor = 0.95 / max_amplitude;
        for sample in &mut mixed {
            *sample *= norm_factor;
        }
        info!("Нормализация микса: max_amplitude={:.6}, коэффициент={:.6}", max_amplitude, norm_factor);
    }
    
    mixed
}

/// Выполняет нормализацию уровня аудио до целевого уровня.
/// 
/// # Аргументы
/// 
/// * `samples` - PCM-семплы для нормализации (изменяются на месте)
/// * `target_level` - Целевой пиковый уровень (от 0.0 до 1.0)
/// 
/// # Возвращает
/// 
/// `true`, если нормализация была выполнена, `false` если образец пуст или содержит только нули
pub fn normalize_peak(samples: &mut [f32], target_level: f32) -> bool {
    if samples.is_empty() {
        return false;
    }
    
    let max_amplitude = samples.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    
    if max_amplitude <= 0.00001 {
        warn!("Аудио содержит только нули или имеет очень низкий уровень: {:.6}", max_amplitude);
        return false;
    }
    
    let norm_factor = target_level / max_amplitude;
    
    for sample in samples.iter_mut() {
        *sample *= norm_factor;
    }
    
    info!("Нормализация пика: max_amplitude={:.6}, целевой уровень={:.6}, коэффициент={:.6}", 
          max_amplitude, target_level, norm_factor);
    true
}

/// Выполняет нормализацию RMS уровня аудио до целевого уровня.
/// 
/// # Аргументы
/// 
/// * `samples` - PCM-семплы для нормализации (изменяются на месте)
/// * `target_rms` - Целевой RMS уровень (обычно 0.1-0.3)
/// 
/// # Возвращает
/// 
/// `true`, если нормализация была выполнена, `false` если образец пуст или содержит только нули
pub fn normalize_rms(samples: &mut [f32], target_rms: f32) -> bool {
    if samples.is_empty() {
        return false;
    }
    
    let current_rms = audio_format::compute_rms(samples);
    
    if current_rms <= 0.00001 {
        warn!("Аудио содержит только нули или имеет очень низкий уровень RMS: {:.6}", current_rms);
        return false;
    }
    
    let norm_factor = target_rms / current_rms;
    
    for sample in samples.iter_mut() {
        *sample *= norm_factor;
    }
    
    // После нормализации RMS проверим, что пики не вышли за пределы
    let max_amplitude = samples.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    if max_amplitude > 1.0 {
        let peak_factor = 0.98 / max_amplitude;
        for sample in samples.iter_mut() {
            *sample *= peak_factor;
        }
        info!("После RMS нормализации потребовалась дополнительная нормализация пиков: {:.6}", peak_factor);
    }
    
    info!("Нормализация RMS: current_rms={:.6}, целевой RMS={:.6}, коэффициент={:.6}", 
          current_rms, target_rms, norm_factor);
    true
}

/// Совмещает два аудиофрагмента с плавным переходом (кроссфейдом).
/// 
/// # Аргументы
/// 
/// * `first` - Первый фрагмент, его конец будет смешан с началом второго
/// * `second` - Второй фрагмент, его начало будет смешано с концом первого
/// * `crossfade_ms` - Длительность кроссфейда в миллисекундах
/// * `sample_rate` - Частота дискретизации
/// 
/// # Возвращает
/// 
/// Объединенные PCM-семплы
pub fn crossfade_fragments(
    first: &[f32],
    second: &[f32],
    crossfade_ms: u32,
    sample_rate: u32
) -> Vec<f32> {
    if first.is_empty() {
        return second.to_vec();
    }
    
    if second.is_empty() {
        return first.to_vec();
    }
    
    let crossfade_samples = ((crossfade_ms as f32 / 1000.0) * sample_rate as f32) as usize;
    
    // Если один из фрагментов короче длины кроссфейда
    if first.len() < crossfade_samples || second.len() < crossfade_samples {
        // Используем простую конкатенацию с fade out/in
        let mut result = first.to_vec();
        result.extend_from_slice(second);
        
        // Применяем fade в местах соединения
        let adjusted_fade = cmp::min(first.len(), second.len()) / 2;
        
        // Fade out в конце первого фрагмента
        for i in 0..adjusted_fade {
            let factor = 1.0 - (i as f32 / adjusted_fade as f32);
            let idx = first.len() - adjusted_fade + i;
            if idx < result.len() {
                result[idx] *= factor;
            }
        }
        
        // Fade in в начале второго фрагмента
        for i in 0..adjusted_fade {
            let factor = i as f32 / adjusted_fade as f32;
            let idx = first.len() + i;
            if idx < result.len() {
                result[idx] *= factor;
            }
        }
        
        return result;
    }
    
    // Результат будет длиной (first.len() + second.len() - crossfade_samples)
    let result_len = first.len() + second.len() - crossfade_samples;
    let mut result = Vec::with_capacity(result_len);
    
    // Копируем первую часть первого фрагмента
    result.extend_from_slice(&first[..first.len() - crossfade_samples]);
    
    // Применяем кроссфейд
    for i in 0..crossfade_samples {
        let first_factor = 1.0 - (i as f32 / crossfade_samples as f32);
        let second_factor = i as f32 / crossfade_samples as f32;
        
        let first_idx = first.len() - crossfade_samples + i;
        let sample = first[first_idx] * first_factor + second[i] * second_factor;
        result.push(sample);
    }
    
    // Копируем оставшуюся часть второго фрагмента
    result.extend_from_slice(&second[crossfade_samples..]);
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_apply_fade() {
        // Создаем тестовый сигнал
        let mut samples = vec![1.0; 1000];
        
        // Применяем fade
        apply_fade(&mut samples, 100, 1000); // 100ms при 1kHz = 100 семплов
        
        // Проверяем fade in
        assert!(samples[0] < 0.01); // Практически ноль в начале
        assert!((samples[50] - 0.5).abs() < 0.01); // Около 0.5 на середине fade in
        assert!(samples[100] > 0.99); // Практически 1.0 после fade in
        
        // Проверяем fade out
        assert!(samples[900] > 0.99); // Практически 1.0 до начала fade out
        assert!((samples[950] - 0.5).abs() < 0.01); // Около 0.5 на середине fade out
        assert!(samples[999] < 0.01); // Практически ноль в конце
    }
    
    #[test]
    fn test_normalize_peak() {
        // Создаем тестовый сигнал с максимальной амплитудой 0.5
        let mut samples = vec![-0.3, 0.1, 0.5, -0.4, 0.2];
        
        // Нормализуем до 0.9
        let result = normalize_peak(&mut samples, 0.9);
        
        // Проверяем результаты
        assert!(result);
        assert!((samples[2] - 0.9).abs() < 0.0001); // Пиковое значение должно быть 0.9
    }
    
    #[test]
    fn test_normalize_rms() {
        // Создаем тестовый сигнал 
        let mut samples = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        
        // Вычисляем текущее RMS
        let current_rms = audio_format::compute_rms(&samples);
        
        // Нормализуем до удвоенного RMS
        let target_rms = current_rms * 2.0;
        let result = normalize_rms(&mut samples, target_rms);
        
        // Проверяем результаты
        assert!(result);
        let new_rms = audio_format::compute_rms(&samples);
        assert!((new_rms - target_rms).abs() < 0.0001);
    }
    
    #[test]
    fn test_crossfade_fragments() {
        // Создаем два тестовых фрагмента
        let first = vec![1.0; 1000];
        let second = vec![0.5; 1000];
        
        // Кроссфейд 100 семплов при 1kHz
        let result = crossfade_fragments(&first, &second, 100, 1000);
        
        // Ожидаемая длина: 1000 + 1000 - 100 = 1900
        assert_eq!(result.len(), 1900);
        
        // Первый семпл должен быть 1.0
        assert!((result[0] - 1.0).abs() < 0.0001);
        
        // Последний семпл должен быть 0.5
        assert!((result[1899] - 0.5).abs() < 0.0001);
        
        // В середине кроссфейда должно быть среднее значение
        let crossfade_mid = 1000 - 50;
        assert!((result[crossfade_mid] - 0.75).abs() < 0.0001);
    }
} 