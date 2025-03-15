//! # TTS Types
//! 
//! Этот модуль содержит общие типы данных и определения ошибок,
//! используемые в компонентах системы генерации и синхронизации TTS.

use std::path::PathBuf;
use tokio::sync::mpsc::Sender;

/// Собственный тип ошибок для библиотеки
#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    #[error("Ошибка парсинга VTT: {0}")]
    VttParsingError(String),
    
    #[error("Ошибка OpenAI API: {0}")]
    OpenAiApiError(String),
    
    #[error("Ошибка HTTP: {0}")]
    HttpError(#[from] reqwest::Error),
    
    #[error("Ошибка аудио-обработки: {0}")]
    AudioProcessingError(String),
    
    #[error("Ошибка time-stretching: {0}")]
    TimeStretchingError(String),
    
    #[error("Ошибка ввода/вывода: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Ошибка WAV-кодирования: {0}")]
    WavEncodingError(#[from] hound::Error),
    
    #[error("Ошибка WAV-декодирования: {0}")]
    WavDecodingError(hound::Error),
    
    #[error("Ошибка конфигурации: {0}")]
    ConfigError(String),
    
    #[error("Другая ошибка: {0}")]
    Other(#[from] anyhow::Error),
}

/// Тип Result для всей TTS библиотеки
pub type Result<T> = std::result::Result<T, TtsError>;

/// Структура для представления одного субтитра из VTT.
#[derive(Clone, Debug)]
pub struct SubtitleCue {
    /// Начальное время в секундах
    pub start: f32,
    /// Конечное время в секундах
    pub end: f32,
    /// Текст субтитра
    pub text: String,
}

/// Структура для хранения фрагмента аудио с метаданными.
#[derive(Debug)]
pub struct AudioFragment {
    /// Семплы PCM (f32) звукового фрагмента
    pub samples: Vec<f32>,
    /// Частота дискретизации (например, 44100, 48000)
    pub sample_rate: u32,
    /// Текст, соответствующий этому фрагменту
    pub text: String,
    /// Начальное время фрагмента в секундах
    pub start_time: f32,
    /// Конечное время фрагмента в секундах
    pub end_time: f32,
    /// Начальное время следующего субтитра (если есть)
    pub next_cue_start: Option<f32>,
}

/// Обновление прогресса для отправки клиенту
#[derive(Debug, Clone)]
pub enum ProgressUpdate {
    /// Началась обработка
    Started,
    /// Парсинг VTT-субтитров
    ParsingVTT,
    /// Генерация TTS для субтитра
    TTSGeneration {
        /// Текущий субтитр
        current: usize,
        /// Общее количество субтитров
        total: usize,
    },
    /// Обработка аудиофрагмента
    ProcessingFragment {
        /// Индекс обрабатываемого фрагмента
        index: usize,
        /// Общее количество фрагментов
        total: usize,
        /// Текущий шаг обработки
        step: String,
    },
    /// Склейка аудиофрагментов
    MergingFragments,
    /// Нормализация аудио
    Normalizing {
        /// Используется ли исходное аудио для нормализации
        using_original: bool,
    },
    /// Кодирование аудио
    Encoding,
    /// Обработка завершена
    Finished,
}

/// Асинхронно отправляет обновление прогресса
pub async fn send_progress(
    sender: &Option<Sender<ProgressUpdate>>,
    update: ProgressUpdate,
) {
    if let Some(sender) = sender {
        let _ = sender.send(update).await;
    }
}

/// Конфигурация голоса для OpenAI TTS
#[derive(Clone, Debug)]
pub struct TtsVoiceConfig {
    /// Идентификатор голоса в API OpenAI
    pub voice: String,
    /// Модель TTS
    pub model: String,
    /// Скорость речи (0.25 до 4.0)
    pub speed: f32,
}

impl Default for TtsVoiceConfig {
    fn default() -> Self {
        Self {
            voice: "alloy".to_string(),
            model: "tts-1".to_string(),
            speed: 1.0,
        }
    }
}

/// Конфигурация обработки аудио
#[derive(Clone, Debug)]
pub struct AudioProcessingConfig {
    /// Целевой пиковый уровень аудио после нормализации
    pub target_peak_level: f32,
    /// Соотношение голоса к инструменталу при микшировании
    pub voice_to_instrumental_ratio: f32,
    /// Усиление инструментальной дорожки
    pub instrumental_boost: f32,
    /// Максимальный фактор ускорения для Rubato
    pub max_rubato_speed: f32,
    /// Максимальный фактор замедления для Rubato
    pub min_rubato_speed: f32,
    /// Максимальный фактор ускорения для SoundTouch
    pub max_soundtouch_speed: f32,
    /// Максимальный фактор замедления для SoundTouch
    pub min_soundtouch_speed: f32,
    /// Насколько можно использовать дополнительное время (если доступно)
    pub extra_time_usage_factor: f32,
}

impl Default for AudioProcessingConfig {
    fn default() -> Self {
        Self {
            target_peak_level: 0.8,
            voice_to_instrumental_ratio: 1.5,
            instrumental_boost: 1.0,
            max_rubato_speed: 1.5,
            min_rubato_speed: 0.5,
            max_soundtouch_speed: 3.0,
            min_soundtouch_speed: 0.3,
            extra_time_usage_factor: 0.3,
        }
    }
}

/// Конфигурация для анализа сегментов
#[derive(Clone, Debug)]
pub struct SegmentAnalysisConfig {
    /// Слов в секунду для нормальной речи
    pub target_words_per_second: f32,
    /// Верхний предел скорости речи (слов в секунду)
    pub max_words_per_second: f32,
}

impl Default for SegmentAnalysisConfig {
    fn default() -> Self {
        Self {
            target_words_per_second: 2.5,
            max_words_per_second: 4.0,
        }
    }
}

/// Результат анализа сегмента субтитров
#[derive(Clone, Debug)]
pub struct SegmentAnalysis {
    /// Индекс сегмента
    pub index: usize,
    /// Количество слов
    pub word_count: usize,
    /// Длительность в секундах
    pub duration: f32,
    /// Слов в секунду
    pub words_per_second: f32,
    /// Требуемый коэффициент ускорения
    pub required_speed_factor: f32,
    /// Критичность проблемы (0-10)
    pub severity: u8,
}

/// Конфигурация синхронизации для процесса генерации TTS
#[derive(Clone)]
pub struct SyncConfig<'a> {
    /// Путь к файлу VTT с субтитрами
    pub vtt_path: &'a str,
    /// Путь для сохранения результирующего WAV-файла
    pub output_wav: PathBuf,
    /// API-ключ OpenAI
    pub api_key: &'a str,
    /// Конфигурация голоса TTS
    pub tts_config: TtsVoiceConfig,
    /// Конфигурация аудио-обработки
    pub audio_config: AudioProcessingConfig,
    /// Путь к оригинальному аудиофайлу (опционально)
    pub original_audio_path: Option<&'a str>,
    /// Отправитель для прогресса выполнения
    pub progress_sender: Option<Sender<ProgressUpdate>>,
} 