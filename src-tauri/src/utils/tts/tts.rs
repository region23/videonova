// lib.rs

//! # TTS Audio Synchronizer Library 
//!
//! Эта библиотека выполняет следующие задачи:
//! 1. Парсинг VTT-субтитров для получения таймингов и текста.
//! 2. Генерация аудиофрагментов через OpenAI TTS API (с параметризируемой конфигурацией).
//! 3. Декодирование аудио в PCM (f32) с помощью symphonia/hound.
//! 4. Корректировка длительности фрагментов с помощью rubato, чтобы итоговая длительность каждого фрагмента стала равной целевому интервалу (без обрезки).
//! 5. Склейка фрагментов с применением fade‑in/fade‑out для устранения щелчков.
//! 6. Нормализация громкости: если указан путь к исходному аудио (mp3/m4a), итоговое аудио приводится к такому же уровню.
//! 7. Кодирование итогового аудио в WAV.
//! 8. Асинхронная передача обновлений прогресса выполнения.
//!
//! ВНИМАНИЕ: Этот файл содержит обратно-совместимое API, использующее новую модульную
//! архитектуру TTS. Для новой разработки рекомендуется использовать новые модули напрямую.

use std::path::{Path, PathBuf};
use tokio::sync::mpsc::Sender;
use log::{info, warn, error};

// Экспортируем типы и структуры из новых модулей для обратной совместимости
pub use crate::utils::tts::types::{
    TtsError, Result, SubtitleCue, AudioFragment, ProgressUpdate, 
    TtsVoiceConfig as TtsConfig, AudioProcessingConfig
};

// А для совместимости со старыми типами добавим определения
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

#[derive(Debug, Clone)]
pub enum DemucsSeparationProgress {
    Started,
    LoadingModel,
    Processing { progress: f32 },
    Converting,
    Finished,
    Error(String),
}

/// Асинхронно отправляет обновление прогресса, если передан канал отправителя
pub async fn send_progress(sender: &Option<Sender<ProgressUpdate>>, update: ProgressUpdate) {
    if let Some(tx) = sender {
        let _ = tx.send(update.clone()).await;
    }
}

// Эта структура обеспечивает обратную совместимость со старой SyncConfig
#[derive(Debug, Clone)]
pub struct SyncConfig<'a> {
    /// API ключ для OpenAI.
    pub api_key: &'a str,
    /// Путь к VTT-файлу с субтитрами.
    pub vtt_path: &'a Path,
    /// Путь для сохранения итогового WAV-файла.
    pub output_wav: &'a Path,
    /// Опциональный путь к исходному аудиофайлу для нормализации громкости (mp3, m4a и т.д.).
    pub original_audio_path: Option<&'a Path>,
    /// Опциональный канал для отправки обновлений прогресса.
    pub progress_sender: Option<Sender<ProgressUpdate>>,
    /// Конфигурация TTS API.
    pub tts_config: TtsConfig,
    /// Конфигурация аудио-обработки.
    pub audio_config: AudioProcessingConfig,
}

impl<'a> SyncConfig<'a> {
    pub fn new(
        api_key: &'a str,
        vtt_path: &'a Path,
        output_wav: &'a Path,
    ) -> Self {
        Self {
            api_key,
            vtt_path,
            output_wav,
            original_audio_path: None,
            progress_sender: None,
            tts_config: TtsConfig::default(),
            audio_config: AudioProcessingConfig::default(),
        }
    }
}

/// Эта функция обеспечивает обратную совместимость с существующим API.
/// Она преобразует старую конфигурацию в новую и вызывает новую функцию синхронизации.
pub async fn process_sync(config: SyncConfig<'_>) -> Result<()> {
    info!("Запуск синхронизации TTS с использованием новой модульной архитектуры...");
    
    // Преобразуем старый формат SyncConfig в новый
    let new_config = crate::utils::tts::types::SyncConfig {
        vtt_path: config.vtt_path.to_str().unwrap_or(""),
        output_wav: config.output_wav.to_path_buf(),
        api_key: config.api_key,
        tts_config: crate::utils::tts::types::TtsVoiceConfig {
            model: config.tts_config.model.clone(),
            voice: config.tts_config.voice.clone(),
            speed: config.tts_config.speed,
        },
        audio_config: config.audio_config.clone(),
        original_audio_path: config.original_audio_path.map(|p| p.to_str().unwrap_or("")),
        progress_sender: config.progress_sender.clone(),
    };
    
    // Вызываем новую функцию синхронизации
    let output_path = crate::utils::tts::synchronize_tts(new_config).await?;
    
    // Проверяем, что выходной файл создан
    if !output_path.exists() {
        return Err(TtsError::AudioProcessingError(
            "Не удалось создать выходной файл".to_string()
        ));
    }
    
    info!("Успешно создан WAV-файл: {}", output_path.display());
    Ok(())
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

/// Работа с OpenAI TTS API.
pub use crate::utils::tts::openai_tts as tts;

/// Функции для аудио-обработки.
pub use crate::utils::tts::audio_format;
pub use crate::utils::tts::audio_processing;

/// Функция для удаления вокала из аудио
pub mod demucs {
    pub use crate::utils::tts::demucs::*;
    
    pub async fn remove_vocals<P: AsRef<std::path::Path>>(
        input_path: P,
        output_path: P,
        progress_sender: Option<tokio::sync::mpsc::Sender<super::DemucsSeparationProgress>>,
        debug_dir: Option<P>,
    ) -> crate::utils::tts::types::Result<()> {
        // Создаем временную директорию для вывода
        let temp_output_dir = if let Some(ref debug_dir) = debug_dir {
            debug_dir.as_ref().to_path_buf()
        } else {
            let output_path = output_path.as_ref();
            output_path.parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join("demucs_temp")
        };
        
        if !temp_output_dir.exists() {
            std::fs::create_dir_all(&temp_output_dir)?;
        }
        
        // Вызываем новую функцию для разделения аудио
        let (instrumental_path, _) = crate::utils::tts::separate_audio(input_path.as_ref().to_path_buf(), temp_output_dir.clone(), None).await?;
        
        // Копируем инструментальный трек в целевой путь
        std::fs::copy(instrumental_path, output_path.as_ref())?;
        
        // Отправляем сообщение о завершении, если есть sender
        if let Some(sender) = progress_sender {
            use super::DemucsSeparationProgress;
            let _ = sender.send(DemucsSeparationProgress::Finished).await;
        }
        
        Ok(())
    }
}

/// Модуль для работы с аудио
pub mod audio {
    pub use crate::utils::tts::audio_format::*;
    pub use crate::utils::tts::audio_processing::*;
    
    // Для обратной совместимости с вызовами remove_vocals из audio модуля
    pub async fn remove_vocals<P: AsRef<std::path::Path>>(
        input_path: P, 
        output_path: P,
        progress_sender: Option<tokio::sync::mpsc::Sender<super::DemucsSeparationProgress>>,
        debug_dir: Option<P>,
    ) -> crate::utils::tts::types::Result<()> {
        super::demucs::remove_vocals(input_path, output_path, progress_sender, debug_dir).await
    }
} 