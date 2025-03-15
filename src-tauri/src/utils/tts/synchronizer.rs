//! # TTS Synchronizer
//! 
//! Центральный модуль для координации процесса генерации и синхронизации TTS.
//! Объединяет функциональность других модулей, обеспечивая работу полного
//! цикла от парсинга субтитров до выходного WAV-файла.

use std::path::PathBuf;
use tokio::sync::mpsc::Sender;
use log::{info, warn};

use crate::utils::tts::types::{
    TtsError, Result, SubtitleCue, AudioFragment, 
    ProgressUpdate, SegmentAnalysisConfig, SyncConfig, send_progress
};
use crate::utils::tts::vtt;
use crate::utils::tts::openai_tts;
use crate::utils::tts::audio_format;
use crate::utils::tts::audio_processing;

/// Структура для управления процессом синхронизации TTS
pub struct TtsSynchronizer {
    /// Конфигурация синхронизации
    config: SyncConfig<'static>,
    /// Буфер для кеширования аудиофрагментов
    fragments_cache: std::collections::HashMap<String, Vec<u8>>,
}

impl TtsSynchronizer {
    /// Создает новый экземпляр синхронизатора
    pub fn new(config: SyncConfig<'static>) -> Self {
        Self {
            config,
            fragments_cache: std::collections::HashMap::new(),
        }
    }
    
    /// Выполняет полный процесс синхронизации TTS
    pub async fn synchronize(&mut self) -> Result<PathBuf> {
        // Отправляем оповещение о начале процесса
        send_progress(&self.config.progress_sender, ProgressUpdate::Started).await;
        
        // Шаг 1: Парсинг VTT-файла
        send_progress(&self.config.progress_sender, ProgressUpdate::ParsingVTT).await;
        let cues = vtt::parse_vtt(self.config.vtt_path)?;
        
        if cues.is_empty() {
            return Err(TtsError::VttParsingError("VTT файл не содержит субтитров".to_string()));
        }
        
        info!("Прочитано {} субтитров из VTT", cues.len());
        
        // Шаг 2: Анализ субтитров для выявления проблем с таймингом
        let segment_config = SegmentAnalysisConfig::default();
        let analysis = vtt::analyze_segments(&cues, &segment_config);
        
        // Шаг 3: Оптимизация распределения времени
        let optimized_cues = vtt::optimize_time_distribution(cues, &analysis);
        
        // Шаг 4: Генерация аудиофрагментов для каждого субтитра
        let fragments = self.generate_fragments(&optimized_cues).await?;
        
        // Шаг 5: Загрузка и анализ оригинального аудио (если указано)
        let original_audio = if let Some(audio_path) = self.config.original_audio_path {
            info!("Загрузка оригинального аудио: {}", audio_path);
            match audio_format::decode_audio_file(audio_path) {
                Ok(audio) => Some(audio),
                Err(err) => {
                    warn!("Не удалось загрузить оригинальное аудио: {}", err);
                    None
                }
            }
        } else {
            None
        };
        
        // Шаг 6: Объединение аудиофрагментов
        send_progress(&self.config.progress_sender, ProgressUpdate::MergingFragments).await;
        let (mut combined_samples, sample_rate) = self.combine_fragments(fragments).await?;
        
        // Шаг 7: Нормализация аудио
        self.normalize_audio(&mut combined_samples, original_audio.as_ref()).await?;
        
        // Шаг 8: Сохранение результата
        send_progress(&self.config.progress_sender, ProgressUpdate::Encoding).await;
        audio_format::encode_wav(&combined_samples, sample_rate, self.config.output_wav.to_str().unwrap())?;
        
        // Шаг 9: Завершение
        send_progress(&self.config.progress_sender, ProgressUpdate::Finished).await;
        info!("Синхронизация TTS завершена успешно");
        
        Ok(self.config.output_wav.clone())
    }
    
    /// Генерирует аудиофрагменты для каждого субтитра
    async fn generate_fragments(&mut self, cues: &[SubtitleCue]) -> Result<Vec<AudioFragment>> {
        let mut fragments = Vec::with_capacity(cues.len());
        let total_cues = cues.len();
        
        for (index, cue) in cues.iter().enumerate() {
            // Отправляем прогресс
            send_progress(
                &self.config.progress_sender, 
                ProgressUpdate::TTSGeneration { current: index + 1, total: total_cues }
            ).await;
            
            info!("Генерация TTS для субтитра {}/{}: '{}'", index + 1, total_cues, cue.text);
            
            // Находим начало следующего субтитра (если есть)
            let next_cue_start = if index < cues.len() - 1 {
                Some(cues[index + 1].start)
            } else {
                None
            };
            
            // Генерируем речь
            let (audio_data, processed_text) = self.generate_tts_for_cue(cue).await?;
            
            // Декодируем MP3 в PCM
            let (samples, sample_rate) = audio_format::decode_mp3(&audio_data)?;
            
            // Создаем аудиофрагмент
            let fragment = AudioFragment {
                samples,
                sample_rate,
                text: processed_text,
                start_time: cue.start,
                end_time: cue.end,
                next_cue_start,
            };
            
            fragments.push(fragment);
        }
        
        Ok(fragments)
    }
    
    /// Генерирует речь для одного субтитра
    async fn generate_tts_for_cue(&mut self, cue: &SubtitleCue) -> Result<(Vec<u8>, String)> {
        // Проверяем кеш
        let cache_key = format!("{}:{}:{}:{}", 
            cue.text, 
            self.config.tts_config.voice, 
            self.config.tts_config.model, 
            self.config.tts_config.speed
        );
        
        if let Some(cached_audio) = self.fragments_cache.get(&cache_key) {
            info!("Используем кешированный TTS для: '{}'", cue.text);
            return Ok((cached_audio.clone(), cue.text.clone()));
        }
        
        // Генерируем речь через OpenAI API
        let result = openai_tts::generate_tts(
            self.config.api_key,
            &cue.text,
            &self.config.tts_config
        ).await?;
        
        // Кешируем результат
        self.fragments_cache.insert(cache_key, result.0.clone());
        
        Ok(result)
    }
    
    /// Объединяет аудиофрагменты в один поток PCM
    async fn combine_fragments(&self, fragments: Vec<AudioFragment>) -> Result<(Vec<f32>, u32)> {
        if fragments.is_empty() {
            return Err(TtsError::AudioProcessingError("Нет аудиофрагментов для объединения".to_string()));
        }
        
        // Получаем sample_rate из первого фрагмента
        let sample_rate = fragments[0].sample_rate;
        
        // Предварительно оцениваем размер выходного буфера
        let estimated_total_samples = fragments.iter()
            .map(|f| f.samples.len())
            .sum::<usize>();
            
        let mut combined = Vec::with_capacity(estimated_total_samples);
        let fade_ms = 20; // Длительность кроссфейда в миллисекундах
        
        // Проходим по всем фрагментам
        for (i, fragment) in fragments.iter().enumerate() {
            send_progress(
                &self.config.progress_sender, 
                ProgressUpdate::ProcessingFragment { 
                    index: i + 1, 
                    total: fragments.len(), 
                    step: "Корректировка длительности".to_string() 
                }
            ).await;
            
            info!("Обработка фрагмента {}/{}: '{}' ({:.2}s - {:.2}s)", 
                i + 1, fragments.len(), fragment.text, fragment.start_time, fragment.end_time);
            
            // Вычисляем длительности
            let target_duration = fragment.end_time - fragment.start_time;
            let actual_duration = fragment.samples.len() as f32 / fragment.sample_rate as f32;
            
            // Определяем доступное дополнительное время
            let available_extra_time = if let Some(next_start) = fragment.next_cue_start {
                // Свободное время до следующего субтитра
                (next_start - fragment.end_time).max(0.0)
            } else {
                // Для последнего фрагмента даем немного свободы
                1.0
            };
            
            // Корректируем длительность аудио
            let (stretched_samples, used_duration) = audio_processing::adjust_duration(
                &fragment.samples,
                actual_duration,
                target_duration,
                available_extra_time,
                fragment.sample_rate,
                &self.config.audio_config
            )?;
            
            info!("Фрагмент {}: исходная длительность {:.3}s → целевая {:.3}s → финальная {:.3}s",
                i + 1, actual_duration, target_duration, used_duration);
            
            // Применяем фейды к фрагменту
            send_progress(
                &self.config.progress_sender, 
                ProgressUpdate::ProcessingFragment { 
                    index: i + 1, 
                    total: fragments.len(), 
                    step: "Применение аудио-эффектов".to_string() 
                }
            ).await;
            
            // Создаем копию для обработки
            let mut processed_samples = stretched_samples.clone();
            audio_processing::apply_fade(&mut processed_samples, fade_ms, fragment.sample_rate);
            
            // Добавляем обработанный фрагмент к выходному потоку
            if i > 0 && !combined.is_empty() {
                // Для всех, кроме первого фрагмента, делаем кроссфейд
                let crossfade_samples = (fragment.sample_rate as u32 * fade_ms / 1000) as usize;
                if combined.len() >= crossfade_samples && processed_samples.len() >= crossfade_samples {
                    // Готовим области для кроссфейда
                    let end_of_previous = combined.len() - crossfade_samples;
                    
                    // Применяем кроссфейд
                    for i in 0..crossfade_samples {
                        let mix_ratio = i as f32 / crossfade_samples as f32;
                        combined[end_of_previous + i] = 
                            combined[end_of_previous + i] * (1.0 - mix_ratio) + 
                            processed_samples[i] * mix_ratio;
                    }
                    
                    // Добавляем оставшуюся часть нового фрагмента
                    combined.extend_from_slice(&processed_samples[crossfade_samples..]);
                } else {
                    // Если фрагменты слишком короткие для кроссфейда, просто добавляем
                    combined.extend_from_slice(&processed_samples);
                }
            } else {
                // Для первого фрагмента просто копируем
                combined.extend_from_slice(&processed_samples);
            }
        }
        
        Ok((combined, sample_rate))
    }
    
    /// Нормализует аудио, возможно используя оригинальное аудио как референс
    async fn normalize_audio(&self, samples: &mut Vec<f32>, original_audio: Option<&(Vec<f32>, u32)>) -> Result<()> {
        if samples.is_empty() {
            return Err(TtsError::AudioProcessingError("Пустой аудиопоток для нормализации".to_string()));
        }
        
        if let Some((ref_samples, _)) = original_audio {
            // Используем оригинальное аудио как референс
            send_progress(
                &self.config.progress_sender, 
                ProgressUpdate::Normalizing { using_original: true }
            ).await;
            
            info!("Нормализация аудио с использованием оригинала как референса");
            
            // Вычисляем RMS оригинала
            let ref_rms = audio_format::compute_rms(ref_samples);
            if ref_rms > 0.0 {
                // Нормализуем с целевым RMS, отражающим оригинал
                let target_rms = ref_rms * 0.9; // Немного тише оригинала
                audio_processing::normalize_rms(samples, target_rms);
                return Ok(());
            }
        }
        
        // Если нет оригинала или он имеет нулевой RMS, используем обычную пиковую нормализацию
        send_progress(
            &self.config.progress_sender, 
            ProgressUpdate::Normalizing { using_original: false }
        ).await;
        
        info!("Стандартная пиковая нормализация аудио");
        let normalized = audio_processing::normalize_peak(samples, self.config.audio_config.target_peak_level)?;
        *samples = normalized;
        
        Ok(())
    }
}

/// Высокоуровневая функция для запуска процесса TTS-синхронизации
pub async fn synchronize_tts(config: SyncConfig<'_>) -> Result<PathBuf> {
    // Преобразуем конфигурацию к статической
    let config_static = SyncConfig {
        vtt_path: Box::leak(config.vtt_path.to_string().into_boxed_str()),
        output_wav: config.output_wav.clone(),
        api_key: Box::leak(config.api_key.to_string().into_boxed_str()),
        tts_config: config.tts_config.clone(),
        audio_config: config.audio_config.clone(),
        original_audio_path: config.original_audio_path.map(|s| {
            Box::leak(s.to_string().into_boxed_str()) as &'static str
        }),
        progress_sender: config.progress_sender.clone(),
    };
    
    let mut synchronizer = TtsSynchronizer::new(config_static);
    synchronizer.synchronize().await
} 