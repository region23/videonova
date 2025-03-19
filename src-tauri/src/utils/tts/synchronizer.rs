//! # TTS Synchronizer
//! 
//! Центральный модуль для координации процесса генерации и синхронизации TTS.
//! Объединяет функциональность других модулей, обеспечивая работу полного
//! цикла от парсинга субтитров до выходного WAV-файла.

use std::path::PathBuf;
use tokio::sync::mpsc::Sender;
use log::{info, warn, error};

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
        
        // Шаг 5: Объединение аудиофрагментов
        send_progress(&self.config.progress_sender, ProgressUpdate::MergingFragments).await;
        let (combined_samples, sample_rate) = self.combine_fragments(fragments).await?;
        
        // Шаг 6: Микширование с инструменталом, если доступен оригинальный аудиофайл
        let (mixed_samples, sample_rate, channels) = self.mix_with_instrumental(&combined_samples, sample_rate).await?;

        // Шаг 7: Нормализация аудио
        let original_audio_path = self.config.original_audio_path;
        send_progress(
            &self.config.progress_sender, 
            ProgressUpdate::Normalizing { using_original: original_audio_path.is_some() }
        ).await;
        
        let normalized = if let Some(path) = original_audio_path {
            // Нормализуем относительно оригинального аудио
            let (original_samples, _) = audio_format::decode_audio_file(path)?;
            let original_rms = audio_format::compute_rms(&original_samples);
            
            if original_rms > 0.00001 {
                // Используем чуть меньшую громкость, чем у оригинала
                let target_rms = original_rms * 0.9;
                let mut mixed_copy = mixed_samples.clone();
                if audio_processing::normalize_rms(&mut mixed_copy, target_rms) {
                    info!("Нормализация аудио с использованием оригинала как референса");
                    mixed_copy
                } else {
                    audio_processing::normalize_peak(&mixed_samples, self.config.audio_config.target_peak_level)?
                }
            } else {
                // Если оригинал слишком тихий, используем стандартную нормализацию
                audio_processing::normalize_peak(&mixed_samples, self.config.audio_config.target_peak_level)?
            }
        } else {
            audio_processing::normalize_peak(&mixed_samples, self.config.audio_config.target_peak_level)?
        };
        
        // Шаг 8: Запись аудио в WAV-файл
        send_progress(&self.config.progress_sender, ProgressUpdate::Encoding).await;
        
        // Записываем аудио с учетом количества каналов (моно или стерео)
        let output_path_str = self.config.output_wav.to_str()
            .ok_or_else(|| TtsError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidInput, 
                "Некорректный путь к выходному файлу"
            )))?;
        
        info!("Сохранение результата в формате с {} каналами", channels);
        audio_format::encode_wav_multi_channel(
            &normalized, 
            sample_rate, 
            channels as u16,  // преобразуем u32 в u16 для совместимости с WavSpec
            output_path_str
        )?;
        
        // Отправляем оповещение о завершении
        send_progress(&self.config.progress_sender, ProgressUpdate::Finished).await;
        
        info!("TTS синхронизация завершена, файл сохранен: {:?}", self.config.output_wav);
        Ok(self.config.output_wav.clone())
    }
    
    /// Генерирует аудиофрагменты для каждого субтитра
    async fn generate_fragments(&mut self, cues: &[SubtitleCue]) -> Result<Vec<AudioFragment>> {
        let total_cues = cues.len();
        info!("Начинаем генерацию TTS для {} субтитров", total_cues);
        
        // Создаем батчи текстов для более эффективной обработки
        let batch_size = 5; // Можно настроить размер батча
        let mut fragments = Vec::with_capacity(total_cues);
        
        // Обрабатываем субтитры батчами
        for chunk in cues.chunks(batch_size) {
            // Индексы для текущего батча
            let start_idx = fragments.len();
            let end_idx = start_idx + chunk.len() - 1;
            
            // Отправляем прогресс
            send_progress(
                &self.config.progress_sender, 
                ProgressUpdate::TTSGeneration { current: start_idx + 1, total: total_cues }
            ).await;
            
            info!("Генерация TTS для батча субтитров {}-{}/{}", 
                  start_idx + 1, end_idx + 1, total_cues);
            
            // Собираем тексты для батча
            let texts: Vec<String> = chunk.iter()
                .map(|cue| cue.text.clone())
                .collect();
            
            // Генерируем TTS для всего батча
            let batch_results = match self.generate_tts_batch_for_cues(&texts).await {
                Ok(results) => results,
                Err(e) => {
                    // Если батчевая обработка не удалась, пытаемся обработать по одному
                    error!("Ошибка при батчевой обработке TTS: {}. Пробуем обработку по одному.", e);
                    
                    let mut individual_results = Vec::with_capacity(chunk.len());
                    
                    for (i, cue) in chunk.iter().enumerate() {
                        let current_idx = start_idx + i;
                        
                        // Отправляем прогресс
                        send_progress(
                            &self.config.progress_sender, 
                            ProgressUpdate::TTSGeneration { current: current_idx + 1, total: total_cues }
                        ).await;
                        
                        info!("Генерация TTS для отдельного субтитра {}/{}: '{}'", 
                             current_idx + 1, total_cues, cue.text);
                        
                        let (audio_data, processed_text) = self.generate_tts_for_cue(cue).await?;
                        individual_results.push((audio_data, processed_text));
                    }
                    
                    individual_results
                }
            };
            
            // Преобразуем полученные аудио данные в AudioFragment
            for (i, (audio_data, processed_text)) in batch_results.into_iter().enumerate() {
                let cue_idx = start_idx + i;
                let cue = &chunk[i];
                
                // Находим начало следующего субтитра (если есть)
                let next_cue_start = if cue_idx < total_cues - 1 {
                    Some(cues[cue_idx + 1].start)
                } else {
                    None
                };
                
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
        }
        
        info!("Завершена генерация TTS для всех {} субтитров", total_cues);
        Ok(fragments)
    }
    
    /// Генерирует речь для батча субтитров
    async fn generate_tts_batch_for_cues(&mut self, texts: &[String]) -> Result<Vec<(Vec<u8>, String)>> {
        // Проверяем батч тестов на пустоту
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        
        // Используем новую функцию батчевой обработки
        openai_tts::generate_tts_batch(
            self.config.api_key,
            texts,
            &self.config.tts_config
        ).await
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
    
    /// Микширует голос TTS с инструментальной дорожкой, если она доступна
    /// Возвращает стерео-микс для лучшего качества звука
    async fn mix_with_instrumental(&self, tts_audio: &[f32], sample_rate: u32) -> Result<(Vec<f32>, u32, u32)> {
        // Проверяем, доступен ли оригинальный аудиофайл для извлечения инструментала
        if let Some(original_audio_path) = self.config.original_audio_path {
            info!("Пробуем получить инструментал из оригинального аудио: {}", original_audio_path);
            
            // Создаем временную директорию для выходных файлов Demucs
            let temp_dir = std::env::temp_dir().join("videonova_demucs");
            if !temp_dir.exists() {
                std::fs::create_dir_all(&temp_dir).map_err(|e| 
                    TtsError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other, 
                        format!("Не удалось создать временную директорию: {}", e)
                    ))
                )?;
            }
            
            // Проверяем, существует ли файл оригинального аудио и имеет ли он размер > 0
            let original_audio_pathbuf = std::path::PathBuf::from(original_audio_path);
            if !original_audio_pathbuf.exists() {
                warn!("Оригинальный аудиофайл не существует: {}. Продолжаем без инструментала.", original_audio_path);
                let stereo_tts = audio_processing::convert_mono_to_stereo(tts_audio);
                return Ok((stereo_tts, sample_rate, 2));
            }
            
            // Проверяем размер файла
            let file_metadata = match std::fs::metadata(&original_audio_pathbuf) {
                Ok(metadata) => metadata,
                Err(e) => {
                    warn!("Не удалось получить метаданные файла: {}. Ошибка: {}. Продолжаем без инструментала.", 
                          original_audio_path, e);
                    let stereo_tts = audio_processing::convert_mono_to_stereo(tts_audio);
                    return Ok((stereo_tts, sample_rate, 2));
                }
            };
            
            if file_metadata.len() == 0 {
                warn!("Оригинальный аудиофайл пуст: {}. Продолжаем без инструментала.", original_audio_path);
                let stereo_tts = audio_processing::convert_mono_to_stereo(tts_audio);
                return Ok((stereo_tts, sample_rate, 2));
            }
            
            // Используем Demucs для разделения аудио на вокал и инструментал
            send_progress(
                &self.config.progress_sender, 
                ProgressUpdate::Custom("Разделение аудио на инструментал и вокал".to_string())
            ).await;
            
            // Этот код может вызвать ошибку, если Demucs не установлен на системе,
            // поэтому заключим его в блок match и продолжим без инструментала, если возникнет ошибка
            let instrumental_path = match crate::utils::tts::demucs::separate_audio(
                original_audio_pathbuf, 
                temp_dir, 
                Some("htdemucs")
            ).await {
                Ok((instrumental_path, _)) => {
                    info!("Успешно извлечен инструментал: {}", instrumental_path.display());
                    instrumental_path
                },
                Err(e) => {
                    warn!("Не удалось извлечь инструментал: {}. Продолжаем без инструментала.", e);
                    // Возвращаем оригинальное TTS аудио в стерео формате
                    let stereo_tts = audio_processing::convert_mono_to_stereo(tts_audio);
                    return Ok((stereo_tts, sample_rate, 2));
                }
            };
            
            // Проверяем, существует ли файл инструментала и имеет ли он размер > 0
            if !instrumental_path.exists() {
                warn!("Файл инструментала не существует: {}. Продолжаем без инструментала.", 
                      instrumental_path.display());
                let stereo_tts = audio_processing::convert_mono_to_stereo(tts_audio);
                return Ok((stereo_tts, sample_rate, 2));
            }
            
            // Проверяем размер файла инструментала
            let inst_metadata = match std::fs::metadata(&instrumental_path) {
                Ok(metadata) => metadata,
                Err(e) => {
                    warn!("Не удалось получить метаданные файла инструментала: {}. Ошибка: {}. Продолжаем без инструментала.", 
                          instrumental_path.display(), e);
                    let stereo_tts = audio_processing::convert_mono_to_stereo(tts_audio);
                    return Ok((stereo_tts, sample_rate, 2));
                }
            };
            
            if inst_metadata.len() == 0 {
                warn!("Файл инструментала пуст: {}. Продолжаем без инструментала.", 
                      instrumental_path.display());
                let stereo_tts = audio_processing::convert_mono_to_stereo(tts_audio);
                return Ok((stereo_tts, sample_rate, 2));
            }
            
            // Выводим расширение файла инструментала для отладки
            let extension = instrumental_path.extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("<нет расширения>");
            info!("Формат файла инструментала: {}, размер: {} байт", 
                 extension, inst_metadata.len());
            
            // Декодируем инструментал
            let (instrumental_samples, instrumental_sample_rate, instrumental_channels) = 
                match audio_format::decode_audio_file_with_channels(&instrumental_path) {
                    Ok(result) => result,
                    Err(e) => {
                        warn!("Не удалось декодировать инструментал: {}. Продолжаем без инструментала.", e);
                        // Возвращаем оригинальное TTS аудио в стерео формате
                        let stereo_tts = audio_processing::convert_mono_to_stereo(tts_audio);
                        return Ok((stereo_tts, sample_rate, 2));
                    }
                };
            
            // Проверяем, не пуст ли результат декодирования
            if instrumental_samples.is_empty() {
                warn!("Декодированные данные инструментала пусты. Продолжаем без инструментала.");
                let stereo_tts = audio_processing::convert_mono_to_stereo(tts_audio);
                return Ok((stereo_tts, sample_rate, 2));
            }
            
            // Ресемплируем инструментал, если необходимо
            let instrumental_samples = if instrumental_sample_rate != sample_rate {
                info!("Ресемплирование инструментала с {} Гц на {} Гц", instrumental_sample_rate, sample_rate);
                // Здесь должен быть код ресемплирования, но пока просто предупреждаем и возвращаем без микширования
                warn!("Разные частоты дискретизации, ресемплирование пока не реализовано");
                // Возвращаем оригинальное TTS аудио в стерео формате
                let stereo_tts = audio_processing::convert_mono_to_stereo(tts_audio);
                return Ok((stereo_tts, sample_rate, 2));
            } else {
                instrumental_samples
            };
            
            // Теперь микшируем голос и инструментал
            send_progress(
                &self.config.progress_sender, 
                ProgressUpdate::Custom("Микширование голоса с инструменталом".to_string())
            ).await;
            
            info!("Микширование TTS с инструменталом");
            
            // Сначала преобразуем моно TTS в стерео, если инструментал стерео
            let tts_stereo = if audio_processing::is_stereo(instrumental_channels) {
                info!("Преобразование моно TTS в стерео для микширования");
                audio_processing::convert_mono_to_stereo(tts_audio)
            } else {
                tts_audio.to_vec()
            };
            
            // Определяем количество каналов в выходном файле - стерео, если инструментал стерео
            let output_channels = if audio_processing::is_stereo(instrumental_channels) { 2 } else { 1 };
            
            // Загружаем параметры из конфигурации
            let voice_level = self.config.audio_config.voice_to_instrumental_ratio;
            let instrumental_level = self.config.audio_config.instrumental_boost;
            
            // Вызываем функцию микширования
            let mixed = audio_processing::mix_audio_tracks(
                &tts_stereo, 
                &instrumental_samples, 
                voice_level, 
                instrumental_level
            )?;
            
            // Проверяем результат
            if mixed.is_empty() {
                warn!("Микширование дало пустой результат. Возвращаем только TTS аудио в стерео.");
                let stereo_tts = audio_processing::convert_mono_to_stereo(tts_audio);
                return Ok((stereo_tts, sample_rate, 2));
            }
            
            info!("Микширование успешно: {} семплов, {} каналов", mixed.len(), output_channels);
            return Ok((mixed, sample_rate, output_channels));
        }
        
        // Если нет оригинального аудио, возвращаем только TTS, но в стерео формате
        let stereo_tts = audio_processing::convert_mono_to_stereo(tts_audio);
        Ok((stereo_tts, sample_rate, 2))
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