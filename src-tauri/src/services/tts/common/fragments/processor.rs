use std::f32::consts::PI;
use super::types::{AudioFragment, FragmentProcessingConfig};
use crate::errors::AppResult;

/// Применяет fade-in эффект к фрагменту
pub fn apply_fade_in(samples: &mut [f32], duration_samples: usize) {
    for i in 0..duration_samples.min(samples.len()) {
        let factor = 0.5 * (1.0 - (PI * i as f32 / duration_samples as f32).cos());
        samples[i] *= factor;
    }
}

/// Применяет fade-out эффект к фрагменту
pub fn apply_fade_out(samples: &mut [f32], duration_samples: usize) {
    let start = samples.len().saturating_sub(duration_samples);
    for i in 0..duration_samples.min(samples.len() - start) {
        let factor = 0.5 * (1.0 + (PI * i as f32 / duration_samples as f32).cos());
        samples[start + i] *= factor;
    }
}

/// Обрабатывает аудио фрагмент
pub fn process_fragment(fragment: &mut AudioFragment, config: &FragmentProcessingConfig) -> AppResult<()> {
    // Вычисляем длительность fade эффектов в сэмплах
    let fade_in_samples = (config.fade_in * fragment.sample_rate as f32) as usize;
    let fade_out_samples = (config.fade_out * fragment.sample_rate as f32) as usize;

    // Применяем fade эффекты
    apply_fade_in(&mut fragment.samples, fade_in_samples);
    apply_fade_out(&mut fragment.samples, fade_out_samples);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_fade_effects() {
        let mut samples = vec![1.0; 1000];
        let fade_samples = 100;

        // Тест fade-in
        apply_fade_in(&mut samples, fade_samples);
        assert!(samples[0] < 0.1); // Начало должно быть близко к нулю
        assert!((samples[fade_samples] - 1.0).abs() < 0.01); // После fade-in должно быть близко к 1.0

        // Тест fade-out
        apply_fade_out(&mut samples, fade_samples);
        assert!((samples[samples.len() - fade_samples] - 1.0).abs() < 0.01); // До fade-out должно быть близко к 1.0
        assert!(samples[samples.len() - 1] < 0.1); // Конец должен быть близко к нулю
    }

    #[test]
    fn test_process_fragment() {
        let mut fragment = AudioFragment::new(
            0,
            Duration::from_secs(0),
            Duration::from_secs(1),
            vec![1.0; 44100],
            44100,
        );

        let config = FragmentProcessingConfig::default();
        process_fragment(&mut fragment, &config).unwrap();

        // Проверяем fade-in
        assert!(fragment.samples[0] < 0.1);
        
        // Проверяем fade-out
        assert!(fragment.samples[fragment.samples.len() - 1] < 0.1);
    }
} 