use crate::utils::tts::audio_processing::{
    apply_fade, adjust_duration, normalize_peak, mix_audio_tracks
};
use crate::utils::tts::soundtouch::process_with_soundtouch;
use crate::utils::tts::types::{AudioProcessingConfig, SubtitleCue};
use std::f32::consts::PI;

/// Создает тестовый синусоидальный сигнал
fn create_sine_wave(freq: f32, duration_sec: f32, sample_rate: u32) -> Vec<f32> {
    let num_samples = (duration_sec * sample_rate as f32) as usize;
    let mut samples = Vec::with_capacity(num_samples);
    
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let amplitude = (2.0 * PI * freq * t).sin();
        samples.push(amplitude);
    }
    
    samples
}

#[test]
fn test_normalize_peak() {
    // Создаем тестовые данные
    let mut samples = vec![0.1, 0.2, 0.5, -0.3, -0.8];
    let target_peak = 0.9;
    
    // Нормализуем образцы
    let result = normalize_peak(&mut samples, target_peak);
    
    // Проверяем, что нормализация прошла успешно
    assert!(result, "Нормализация должна вернуть true");
    
    // Максимальная амплитуда в исходном сигнале была 0.8
    // После нормализации она должна стать равной target_peak
    let max_amplitude = samples.iter().map(|&s| s.abs()).fold(0.0, f32::max);
    assert!((max_amplitude - target_peak).abs() < 1e-6, 
            "Максимальная амплитуда должна быть приближенно равна целевому значению");
    
    // Проверяем отношения между амплитудами (они должны сохраниться)
    assert!((samples[2] / samples[1] - 2.5).abs() < 1e-6, 
            "Отношения между амплитудами должны сохраняться");
}

#[test]
fn test_apply_fade() {
    // Создаем тестовый сигнал
    let mut samples = create_sine_wave(440.0, 1.0, 44100);
    let fade_samples = 4410; // 0.1 секунды при 44100 Гц
    
    // Сохраняем оригинальные образцы для сравнения
    let original = samples.clone();
    
    // Применяем fade in/out
    apply_fade(&mut samples, 100, 44100); // 100 мс fade
    
    // Проверяем, что первый образец стал ближе к нулю (fade-in)
    assert!(samples[0].abs() < original[0].abs(), 
            "Первый образец должен быть ближе к нулю после применения fade-in");
    
    // Проверяем, что последний образец стал ближе к нулю (fade-out)
    assert!(samples[samples.len() - 1].abs() < original[original.len() - 1].abs(),
           "Последний образец должен быть ближе к нулю после применения fade-out");
    
    // Проверяем, что средние образцы не изменились
    let mid_point = samples.len() / 2;
    assert!((samples[mid_point] - original[mid_point]).abs() < 1e-6,
           "Средние образцы не должны изменяться");
}

#[test]
fn test_adjust_duration() {
    // Создаем тестовые параметры
    let input_samples = create_sine_wave(440.0, 2.0, 44100); // 2 секунды аудио
    let actual_duration = 2.0; // секунды
    let target_duration = 1.8; // секунды (немного сжимаем)
    let available_extra_time = 0.5; // дополнительное доступное время
    let sample_rate = 44100;
    
    let config = AudioProcessingConfig {
        max_rubato_speed: 2.0,
        min_rubato_speed: 0.5,
        max_soundtouch_speed: 2.0,
        min_soundtouch_speed: 0.5,
        extra_time_usage_factor: 0.7,
        ..AudioProcessingConfig::default()
    };
    
    // Изменяем длительность
    let result = adjust_duration(
        &input_samples, 
        actual_duration, 
        target_duration, 
        available_extra_time,
        sample_rate,
        &config
    );
    
    // Проверяем результат
    assert!(result.is_ok(), "Функция adjust_duration должна выполняться без ошибок");
    
    if let Ok((adjusted_samples, new_duration)) = result {
        // Проверяем, что длительность изменилась корректно
        let expected_sample_count = (target_duration * sample_rate as f32) as usize;
        let tolerance = (expected_sample_count as f32 * 0.05) as usize; // 5% погрешность
        
        assert!(
            adjusted_samples.len() >= expected_sample_count - tolerance && 
            adjusted_samples.len() <= expected_sample_count + tolerance,
            "Ожидаемая длина: {}, фактическая: {}", 
            expected_sample_count, adjusted_samples.len()
        );
        
        // Проверяем, что возвращенная длительность соответствует ожидаемой
        assert!(
            (new_duration - target_duration).abs() < 0.1,
            "Ожидаемая длительность: {}, фактическая: {}", 
            target_duration, new_duration
        );
    }
}

#[test]
fn test_mix_audio_tracks() {
    // Создаем две тестовые дорожки
    let voice_track = create_sine_wave(440.0, 1.0, 44100); // голос
    let instrumental_track = create_sine_wave(220.0, 1.0, 44100); // инструментал
    
    // Параметры микширования
    let voice_to_instrumental_ratio = 1.5; // голос в 1.5 раза громче
    let instrumental_boost = 1.2; // небольшое усиление инструментала
    
    // Микшируем треки
    let mixed = mix_audio_tracks(
        &voice_track, 
        &instrumental_track, 
        voice_to_instrumental_ratio,
        instrumental_boost
    );
    
    // Проверяем, что результат имеет правильную длину
    assert_eq!(mixed.len(), voice_track.len(), "Длина микса должна соответствовать длине треков");
    
    // Проверяем, что амплитуда в разумных пределах
    let max_amplitude = mixed.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    assert!(max_amplitude <= 1.0, "Максимальная амплитуда микса не должна превышать 1.0");
}

// Тест для SoundTouch, который требует установленной библиотеки
#[test]
#[ignore = "Требует доступа к SoundTouch, может быть недоступно в CI/CD"]
fn test_process_with_soundtouch() {
    // Создаем тестовый сигнал
    let samples = create_sine_wave(440.0, 1.0, 44100);
    let sample_rate = 44100;
    let tempo = 0.5; // замедляем в два раза
    
    // Растягиваем сигнал
    let result = process_with_soundtouch(&samples, sample_rate, tempo);
    
    // Проверяем результат
    assert!(result.is_ok(), "Функция process_with_soundtouch должна выполняться без ошибок");
    
    if let Ok(stretched) = result {
        // Растянутый сигнал должен быть примерно в 2 раза длиннее
        let expected_length = (samples.len() as f32 / tempo) as usize;
        let tolerance = (expected_length as f32 * 0.1) as usize; // 10% погрешность
        
        assert!(
            stretched.len() >= expected_length - tolerance && 
            stretched.len() <= expected_length + tolerance,
            "Ожидаемая длина: {}, фактическая: {}", 
            expected_length, stretched.len()
        );
    }
} 