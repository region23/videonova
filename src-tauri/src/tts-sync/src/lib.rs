//! Основной файл библиотеки tts-sync с поддержкой системы прогресса и уведомлений
//! 
//! Эта библиотека предоставляет инструменты для синхронизации TTS с видео,
//! с возможностью отслеживания прогресса выполнения операций.

pub mod progress;
pub mod notification;
pub mod config;
pub mod error;
pub mod subtitle;
pub mod tts;
pub mod media;
pub mod utils;

use std::path::Path;
use anyhow::Result;
use crate::error::TtsSyncError;
use crate::config::TtsSyncConfig;
use crate::progress::{ProgressTracker, ProgressObserver, ProgressReporter, ProcessStep};

/// Основная структура для работы с библиотекой
pub struct TtsSync {
    /// Конфигурация библиотеки
    config: TtsSyncConfig,
    /// Трекер прогресса
    progress_tracker: Option<ProgressTracker>,
}

impl TtsSync {
    /// Создать новый экземпляр TtsSync с указанной конфигурацией
    pub fn new(config: TtsSyncConfig) -> Self {
        Self {
            config,
            progress_tracker: None,
        }
    }
    
    /// Создать новый экземпляр TtsSync с указанной конфигурацией и репортером прогресса
    pub fn with_progress_reporter(config: TtsSyncConfig, reporter: Box<dyn ProgressReporter>) -> Self {
        let mut tracker = ProgressTracker::new();
        tracker.set_reporter(reporter);
        
        Self {
            config,
            progress_tracker: Some(tracker),
        }
    }
    
    /// Создать экземпляр TtsSync с настройками по умолчанию
    pub fn default() -> Self {
        Self::new(TtsSyncConfig::default())
    }
    
    /// Установить репортер прогресса
    pub fn set_progress_reporter(&mut self, reporter: Box<dyn ProgressReporter>) {
        if let Some(tracker) = &mut self.progress_tracker {
            tracker.set_reporter(reporter);
        } else {
            let mut tracker = ProgressTracker::new();
            tracker.set_reporter(reporter);
            self.progress_tracker = Some(tracker);
        }
    }
    
    /// Добавить наблюдателя прогресса
    pub fn add_observer(&mut self, observer: Box<dyn ProgressObserver>) -> Result<usize, TtsSyncError> {
        if let Some(tracker) = &mut self.progress_tracker {
            Ok(tracker.add_observer(observer).unwrap_or(0))
        } else {
            let mut tracker = ProgressTracker::new();
            let id = tracker.add_observer(observer).unwrap_or(0);
            self.progress_tracker = Some(tracker);
            Ok(id)
        }
    }
    
    /// Основной метод для обработки и синхронизации TTS
    pub async fn process(
        &self,
        video_path: &str,
        audio_path: &str,
        original_vtt_path: &str,
        translated_vtt_path: &str,
        output_path: &str,
    ) -> Result<String, TtsSyncError> {
        log::info!("Starting TTS synchronization process");
        
        // Проверяем наличие трекера прогресса
        let tracker_ref = self.progress_tracker.as_ref();
        
        // Validate input files
        log::info!("Validating input files");
        if let Some(t) = tracker_ref {
            t.set_step(ProcessStep::SubtitleParsing);
            t.update_step_progress(0.0, Some("Проверка входных файлов".to_string()));
        }

        for (file_path, description) in [
            (video_path, "video"),
            (audio_path, "audio"),
            (original_vtt_path, "original subtitles"),
            (translated_vtt_path, "translated subtitles"),
        ] {
            if !tokio::fs::metadata(file_path).await.is_ok() {
                let error = format!("Input {} file not found: {}", description, file_path);
                log::error!("{}", error);
                return Err(TtsSyncError::FileNotFound(error));
            }
        }
        
        // 1. Парсинг и анализ субтитров
        if let Some(t) = tracker_ref {
            t.update_step_progress(10.0, Some("Начало парсинга субтитров".to_string()));
        }
        
        let original_subtitles = subtitle::parser::parse_vtt_file(original_vtt_path)
            .map_err(|e| TtsSyncError::SubtitleParsing(e.to_string()))?;
        
        if let Some(t) = tracker_ref {
            t.update_step_progress(50.0, Some("Парсинг оригинальных субтитров завершен".to_string()));
        }
        
        let translated_subtitles = subtitle::parser::parse_vtt_file(translated_vtt_path)
            .map_err(|e| TtsSyncError::SubtitleParsing(e.to_string()))?;
        
        if let Some(t) = tracker_ref {
            t.update_step_progress(100.0, Some("Парсинг субтитров завершен".to_string()));
        }
        
        // 2. Анализ временных меток
        if let Some(t) = tracker_ref {
            t.set_step(ProcessStep::TimingAnalysis);
            t.update_step_progress(0.0, Some("Начало анализа временных меток".to_string()));
        }
        
        let metrics = subtitle::analyzer::analyze_subtitle_timing(&original_subtitles);
        
        if let Some(t) = tracker_ref {
            t.update_step_progress(100.0, Some("Анализ временных меток завершен".to_string()));
        }
        
        // 3. Оптимизация субтитров для TTS
        if let Some(t) = tracker_ref {
            t.set_step(ProcessStep::SubtitleOptimization);
            t.update_step_progress(0.0, Some("Начало оптимизации субтитров".to_string()));
        }
        
        let processed_subtitles = subtitle::optimizer::optimize_for_tts(
            &translated_subtitles,
            &original_subtitles,
            &metrics,
            &self.config,
        ).map_err(|e| TtsSyncError::SubtitleProcessing(e.to_string()))?;
        
        if let Some(t) = tracker_ref {
            t.update_step_progress(100.0, Some("Оптимизация субтитров завершена".to_string()));
        }
        
        // 4. Генерация речи с использованием OpenAI API
        if let Some(t) = tracker_ref {
            t.set_step(ProcessStep::SpeechGeneration);
        }
        
        let tts_audio_path = tts::openai::generate_speech_with_progress(
            &processed_subtitles,
            &self.config,
            tracker_ref,
        ).await.map_err(|e| {
            log::error!("TTS generation failed: {}", e);
            TtsSyncError::TtsGeneration(e.to_string())
        })?;
        
        // 5. Синхронизация аудио с видео
        if let Some(t) = tracker_ref {
            t.set_step(ProcessStep::AudioVideoSync);
            t.update_step_progress(0.0, Some("Начало синхронизации аудио с видео".to_string()));
        }
        
        let output_file = media::sync::create_multi_track_video(
            video_path,
            audio_path,
            &tts_audio_path,
            translated_vtt_path,
            output_path,
            &self.config,
        ).map_err(|e| {
            log::error!("Audio-video synchronization failed: {}", e);
            TtsSyncError::Synchronization(e.to_string())
        })?;
        
        if let Some(t) = tracker_ref {
            t.update_step_progress(100.0, Some("Синхронизация аудио с видео завершена".to_string()));
            t.complete();
        }
        
        log::info!("TTS synchronization completed successfully");
        Ok(output_file)
    }
}

/// Публичный API для удобного использования
pub async fn synchronize_tts(
    video_path: &str,
    audio_path: &str,
    original_vtt_path: &str,
    translated_vtt_path: &str,
    output_path: &str,
    openai_api_key: &str,
) -> Result<String, TtsSyncError> {
    let config = TtsSyncConfig {
        openai_api_key: openai_api_key.to_string(),
        ..TtsSyncConfig::default()
    };
    
    let tts_sync = TtsSync::new(config);
    tts_sync.process(
        video_path,
        audio_path,
        original_vtt_path,
        translated_vtt_path,
        output_path,
    ).await
}

/// Публичный API с поддержкой отслеживания прогресса
pub async fn synchronize_tts_with_progress(
    video_path: &str,
    audio_path: &str,
    original_vtt_path: &str,
    translated_vtt_path: &str,
    output_path: &str,
    openai_api_key: &str,
    reporter: Box<dyn ProgressReporter>,
) -> Result<String, TtsSyncError> {
    let config = TtsSyncConfig {
        openai_api_key: openai_api_key.to_string(),
        ..TtsSyncConfig::default()
    };
    
    let tts_sync = TtsSync::with_progress_reporter(config, reporter);
    tts_sync.process(
        video_path,
        audio_path,
        original_vtt_path,
        translated_vtt_path,
        output_path,
    ).await
}
