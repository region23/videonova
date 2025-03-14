use std::time::Duration;

/// Аудио фрагмент
#[derive(Clone, Debug)]
pub struct AudioFragment {
    /// Индекс фрагмента
    pub index: usize,
    /// Время начала
    pub start: Duration,
    /// Время окончания
    pub end: Duration,
    /// Аудио данные (PCM, 32-bit float, mono)
    pub samples: Vec<f32>,
    /// Частота дискретизации
    pub sample_rate: u32,
}

impl AudioFragment {
    /// Создает новый аудио фрагмент
    pub fn new(
        index: usize,
        start: Duration,
        end: Duration,
        samples: Vec<f32>,
        sample_rate: u32,
    ) -> Self {
        Self {
            index,
            start,
            end,
            samples,
            sample_rate,
        }
    }

    /// Получает длительность фрагмента
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }

    /// Получает длительность в сэмплах
    pub fn samples_duration(&self) -> usize {
        self.samples.len()
    }
}

/// Параметры обработки фрагментов
#[derive(Clone, Debug)]
pub struct FragmentProcessingConfig {
    /// Длительность fade-in (в секундах)
    pub fade_in: f32,
    /// Длительность fade-out (в секундах)
    pub fade_out: f32,
    /// Целевой уровень громкости (в dB)
    pub target_level: f32,
}

impl Default for FragmentProcessingConfig {
    fn default() -> Self {
        Self {
            fade_in: 0.02,  // 20ms
            fade_out: 0.02, // 20ms
            target_level: -14.0, // -14 dB LUFS
        }
    }
} 