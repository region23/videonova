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
//! **Замечание:** Для полноценного использования потребуется доработка обработки ошибок и параметризация DSP‑алгоритмов.

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
    
    #[error("Ошибка конфигурации: {0}")]
    #[allow(dead_code)]
    ConfigError(String),
    
    #[error("Другая ошибка: {0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, TtsError>;

/// Структура для представления одного субтитра из VTT.
#[derive(Clone, Debug)]
pub struct SubtitleCue {
    pub start: f32,   // время начала в секундах
    pub end: f32,     // время окончания в секундах
    pub text: String, // текст реплики
}

/// Тип обновления прогресса выполнения.
#[derive(Debug)]
pub enum ProgressUpdate {
    Started,
    ParsingVTT,
    ParsedVTT { total: usize },
    TTSGeneration { current: usize, total: usize },
    ProcessingFragment { index: usize, total: usize, step: String },
    MergingFragments,
    Normalizing { using_original: bool },
    Encoding,
    Finished,
}

/// Конфигурация для TTS API
#[derive(Debug, Clone)]
pub struct TtsConfig {
    /// Модель TTS, например "tts-1-hd"
    pub model: String,
    /// Голос, например "alloy", "echo", "fable" и т.д.
    pub voice: String,
    /// Скорость речи (0.5 - 2.0)
    pub speed: f32,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            model: "tts-1-hd".to_string(),
            voice: "alloy".to_string(),
            speed: 1.0,
        }
    }
}

/// Конфигурация для аудио-обработки
#[derive(Debug, Clone)]
pub struct AudioProcessingConfig {
    /// Длительность fade-in и fade-out в миллисекундах
    pub fade_ms: u32,
    /// Минимальный фактор замедления (0.1 - 1.0)
    pub min_slowdown_factor: f32,
    /// Размер окна для FFT при time-stretching
    pub window_size: usize,
    /// Размер перекрытия для FFT при time-stretching
    pub hop_size: usize,
    /// Целевой уровень нормализации при отсутствии референсного аудио
    pub target_peak_level: f32,
}

impl Default for AudioProcessingConfig {
    fn default() -> Self {
        Self {
            fade_ms: 20,
            min_slowdown_factor: 0.9,
            window_size: 1024,
            hop_size: 512,
            target_peak_level: 0.9,
        }
    }
}

/// Модуль для парсинга VTT-файлов.
pub mod vtt {
    use super::{SubtitleCue, Result, TtsError};
    use std::fs;

    /// Парсит VTT-файл и возвращает вектор структур SubtitleCue.
    pub fn parse_vtt<P: AsRef<std::path::Path>>(file_path: P) -> Result<Vec<SubtitleCue>> {
        let data = fs::read_to_string(file_path)
            .map_err(|e| TtsError::IoError(e))?;
        let mut cues = Vec::new();

        // Разбиваем файл на блоки по пустой строке
        for block in data.split("\n\n").filter(|b| b.contains("-->")) {
            let mut lines = block.lines();
            // Пропускаем возможный индекс или метку
            let timing_line = lines.find(|l| l.contains("-->"))
                .ok_or_else(|| TtsError::VttParsingError("Не найден тайминг в блоке".to_string()))?;
            let times: Vec<&str> = timing_line.split_whitespace().collect();
            if times.len() >= 3 && times[1] == "-->" {
                let start = parse_time(times[0])?;
                let end = parse_time(times[2])?;
                // Оставшиеся строки считаем текстом реплики
                let text = lines.collect::<Vec<_>>().join(" ");
                cues.push(SubtitleCue { start, end, text });
            }
        }
        Ok(cues)
    }

    /// Преобразует строку времени формата "HH:MM:SS.mmm" в секунды.
    fn parse_time(t: &str) -> Result<f32> {
        let parts: Vec<&str> = t.split(|c| c == ':' || c == '.').collect();
        if parts.len() < 3 {
            return Err(TtsError::VttParsingError(format!("Неверный формат времени: {}", t)));
        }
        
        let hours: f32 = parts[0].parse()
            .map_err(|_| TtsError::VttParsingError(format!("Не удалось распознать часы: {}", parts[0])))?;
        let minutes: f32 = parts[1].parse()
            .map_err(|_| TtsError::VttParsingError(format!("Не удалось распознать минуты: {}", parts[1])))?;
        let seconds: f32 = parts[2].parse()
            .map_err(|_| TtsError::VttParsingError(format!("Не удалось распознать секунды: {}", parts[2])))?;
        let millis: f32 = if parts.len() > 3 { 
            parts[3].parse()
                .map_err(|_| TtsError::VttParsingError(format!("Не удалось распознать миллисекунды: {}", parts[3])))?
        } else { 
            0.0 
        };
        
        Ok(hours * 3600.0 + minutes * 60.0 + seconds + millis / 1000.0)
    }
}

/// Модуль для обращения к OpenAI TTS API.
pub mod tts {
    use super::{Result, TtsError, TtsConfig};
    use reqwest::Client;
    use serde_json::json;

    /// Генерирует аудиофрагмент через TTS API для заданного текста.
    /// Возвращает Vec<u8> с данными аудио (например, WAV).
    pub async fn generate_tts(api_key: &str, text: &str, config: &TtsConfig) -> Result<Vec<u8>> {
        let payload = json!({
            "model": config.model,
            "voice": config.voice,
            "input": text,
            "response_format": "wav",
            "speed": config.speed
        });

        let client = Client::new();
        let resp = client
            .post("https://api.openai.com/v1/audio/speech")
            .bearer_auth(api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|e| TtsError::HttpError(e))?;
            
        if !resp.status().is_success() {
            let status = resp.status();
            let error_text = resp.text().await.unwrap_or_else(|_| "Неизвестная ошибка".to_string());
            return Err(TtsError::OpenAiApiError(format!(
                "Ошибка API (код {}): {}", status, error_text
            )));
        }
        
        let audio_bytes = resp.bytes().await
            .map_err(|e| TtsError::HttpError(e))?;
        Ok(audio_bytes.to_vec())
    }
}

/// Модуль для аудио-обработки: декодирование, time-stretching, анализ громкости и кодирование.
pub mod audio {
    use super::{Result, TtsError, AudioProcessingConfig};
    use rubato::{FftFixedIn, Resampler};
    use std::io::Cursor;

    // Для декодирования/кодирования WAV используем hound.
    use hound;

    // Для декодирования исходного аудио (mp3/m4a) используем symphonia.
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    use symphonia::default::get_probe;
    use std::fs::File;
    use std::path::Path;

    /// Декодирует WAV-данные из Vec<u8> в вектор f32-сэмплов (моно).
    /// Возвращает сэмплы и частоту дискретизации.
    pub fn decode_wav(data: &[u8]) -> Result<(Vec<f32>, u32)> {
        let mut reader = hound::WavReader::new(Cursor::new(data))
            .map_err(|e| TtsError::AudioProcessingError(format!("Не удалось создать WAV-ридер: {}", e)))?;
        let spec = reader.spec();
        let sample_rate = spec.sample_rate;
        let samples: Vec<f32> = reader.samples::<i16>()
            .map(|s| s.map_err(|e| TtsError::AudioProcessingError(format!("Ошибка чтения сэмпла: {}", e)))
                 .map(|val| val as f32 / i16::MAX as f32))
            .collect::<Result<Vec<f32>>>()?;
        Ok((samples, sample_rate))
    }

    /// Кодирует вектор f32-сэмплов (моно) в WAV-формат.
    pub fn encode_wav(samples: &[f32], sample_rate: u32, output_path: &str) -> Result<()> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(output_path, spec)
            .map_err(|e| TtsError::WavEncodingError(e))?;
        for &sample in samples {
            let s = (sample * i16::MAX as f32) as i16;
            writer.write_sample(s)
                .map_err(|e| TtsError::WavEncodingError(e))?;
        }
        writer.finalize()
            .map_err(|e| TtsError::WavEncodingError(e))?;
        Ok(())
    }

    /// Вычисляет длительность аудио по количеству сэмплов и частоте дискретизации.
    pub fn duration_in_seconds(num_samples: usize, sample_rate: u32) -> f32 {
        num_samples as f32 / sample_rate as f32
    }

    /// Применяет time-stretching к аудио для корректировки длительности.
    ///
    /// Если actual_duration > target_duration, вычисляется коэффициент ускорения:
    /// speed_factor = actual_duration / target_duration (без ограничения сверху),
    /// затем rubato ускоряет аудио, чтобы итоговая длительность приблизилась к target_duration.
    ///
    /// Если actual_duration < target_duration, сначала пытаемся замедлить до min_slowdown_factor,
    /// а затем, если нужно, добавляем тишину.
    pub fn adjust_duration(
        input: &[f32],
        actual_duration: f32,
        target_duration: f32,
        sample_rate: u32,
        config: &AudioProcessingConfig,
    ) -> Result<Vec<f32>> {
        if actual_duration > target_duration {
            let speed_factor = actual_duration / target_duration;
            let input_sample_rate = sample_rate as usize;
            let output_sample_rate = (input_sample_rate as f32 / speed_factor) as usize;
            
            let mut resampler = FftFixedIn::<f32>::new(
                input_sample_rate,
                output_sample_rate,
                config.window_size,
                config.hop_size,
                1,
            )
            .map_err(|e| TtsError::TimeStretchingError(e.to_string()))?;

            let input_channels = vec![input.to_vec()];
            let processed = resampler
                .process(&input_channels, None)
                .map_err(|e| TtsError::TimeStretchingError(e.to_string()))?;
            Ok(processed[0].clone())
        } else {
            let computed_factor = actual_duration / target_duration;
            let speed_factor = computed_factor.max(config.min_slowdown_factor);
            
            let input_sample_rate = sample_rate as usize;
            let output_sample_rate = (input_sample_rate as f32 / speed_factor) as usize;
            
            let mut resampler = FftFixedIn::<f32>::new(
                input_sample_rate,
                output_sample_rate,
                config.window_size,
                config.hop_size,
                1,
            )
            .map_err(|e| TtsError::TimeStretchingError(e.to_string()))?;
            
            let mut output = resampler
                .process(&vec![input.to_vec()], None)
                .map_err(|e| TtsError::TimeStretchingError(e.to_string()))?[0]
                .clone();

            let target_samples = (target_duration * sample_rate as f32).round() as usize;
            if output.len() < target_samples {
                output.extend(vec![0.0; target_samples - output.len()]);
            }
            Ok(output)
        }
    }

    /// Применяет короткие fade-in и fade-out (в миллисекундах) к аудиофрагменту для сглаживания границ.
    pub fn apply_fades(input: &[f32], sample_rate: u32, fade_ms: u32) -> Vec<f32> {
        let fade_samples = (sample_rate as f32 * fade_ms as f32 / 1000.0).round() as usize;
        let mut output = input.to_vec();

        // Применяем fade-in
        for i in 0..fade_samples.min(output.len()) {
            let factor = i as f32 / fade_samples as f32;
            output[i] *= factor;
        }
        
        // Применяем fade-out
        for i in 0..fade_samples.min(output.len()) {
            let idx = output.len() - 1 - i;
            let factor = i as f32 / fade_samples as f32;
            output[idx] *= factor;
        }
        
        output
    }

    /// Декодирует исходный аудиофайл (mp3, m4a и др.) с помощью symphonia и возвращает PCM-сэмплы и sample_rate.
    pub fn decode_audio_file<P: AsRef<Path>>(path: P) -> Result<(Vec<f32>, u32)> {
        let file = File::open(path.as_ref())
            .map_err(|e| TtsError::IoError(e))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.as_ref().extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        
        let probed = get_probe().format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default()
        ).map_err(|e| TtsError::AudioProcessingError(format!("Не удалось определить формат исходного аудио: {}", e)))?;
        
        let mut format = probed.format;
        let track = format.default_track()
            .ok_or_else(|| TtsError::AudioProcessingError("Не найден аудиотрек".to_string()))?;
        
        let dec_opts = DecoderOptions::default();
        let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &dec_opts)
            .map_err(|e| TtsError::AudioProcessingError(format!("Не удалось создать декодер: {}", e)))?;
        
        let mut sample_buf: Option<SampleBuffer<f32>> = None;
        let mut samples = Vec::new();
        let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
        
        while let Ok(packet) = format.next_packet() {
            let decoded = decoder.decode(&packet)
                .map_err(|e| TtsError::AudioProcessingError(format!("Ошибка декодирования: {}", e)))?;
            
            if sample_buf.is_none() {
                sample_buf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec()));
            }
            
            if let Some(buf) = &mut sample_buf {
                buf.copy_interleaved_ref(decoded);
                samples.extend_from_slice(buf.samples());
            }
        }
        
        Ok((samples, sample_rate))
    }

    /// Вычисляет RMS-уровень (корень из среднего квадрата) для набора сэмплов.
    pub fn compute_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }
}

/// Основной API библиотеки.
pub mod synchronizer {
    use super::{audio, tts, vtt, ProgressUpdate, Result, TtsError, TtsConfig, AudioProcessingConfig};
    use futures::future::join_all;
    use std::path::Path;
    use tokio::sync::mpsc::Sender;

    /// Параметры для синхронизации.
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
        /// Создает новую конфигурацию с дефолтными значениями для TTS и аудио-обработки
        #[allow(dead_code)]
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

    /// Отправляет сообщение о прогрессе, если канал присутствует.
    async fn send_progress(sender: &Option<Sender<ProgressUpdate>>, update: ProgressUpdate) {
        if let Some(tx) = sender {
            let _ = tx.send(update).await;
        }
    }

    /// Выполняет полный процесс синхронизации:
    /// - Парсинг VTT
    /// - Генерация аудио через TTS API
    /// - Декодирование, корректировка длительности, применение fade‑in/fade‑out для каждого аудиофрагмента
    /// - Склейка фрагментов, нормализация громкости (если указан оригинальный аудиофайл), запись итогового аудио в WAV.
    pub async fn process_sync(config: SyncConfig<'_>) -> Result<()> {
        send_progress(&config.progress_sender, ProgressUpdate::Started).await;

        // 1. Парсинг VTT
        send_progress(&config.progress_sender, ProgressUpdate::ParsingVTT).await;
        let cues = vtt::parse_vtt(&config.vtt_path)?;
        send_progress(&config.progress_sender, ProgressUpdate::ParsedVTT { total: cues.len() }).await;
        println!("Найдено {} реплик", cues.len());

        // 2. Генерация TTS для каждой реплики параллельно
        let tts_futures = cues.iter().enumerate().map(|(i, cue)| {
            let api_key = config.api_key;
            let text = cue.text.clone();
            let tts_config = &config.tts_config;
            async move {
                let res = tts::generate_tts(api_key, &text, tts_config).await;
                (i, res)
            }
        });
        let tts_results = join_all(tts_futures).await;
        let mut audio_fragments = Vec::new();

        // 3. Обработка каждого аудиофрагмента
        for (i, (cue, tts_result)) in cues.iter().zip(tts_results.into_iter()).enumerate() {
            send_progress(&config.progress_sender, ProgressUpdate::TTSGeneration { current: i + 1, total: cues.len() }).await;
            let audio_bytes = tts_result.1?;
            let (pcm, sample_rate) = audio::decode_wav(&audio_bytes)?;
            let actual_duration = audio::duration_in_seconds(pcm.len(), sample_rate);
            let target_duration = cue.end - cue.start;
            send_progress(
                &config.progress_sender,
                ProgressUpdate::ProcessingFragment {
                    index: i + 1,
                    total: cues.len(),
                    step: format!("Длительность: target {:.3} s, actual {:.3} s", target_duration, actual_duration),
                },
            )
            .await;
            
            let adjusted = audio::adjust_duration(
                &pcm, 
                actual_duration, 
                target_duration, 
                sample_rate, 
                &config.audio_config
            )?;
            
            let faded = audio::apply_fades(&adjusted, sample_rate, config.audio_config.fade_ms);
            audio_fragments.push((faded, sample_rate));
        }

        // 4. Склейка аудиофрагментов
        send_progress(&config.progress_sender, ProgressUpdate::MergingFragments).await;
        if audio_fragments.is_empty() {
            return Err(TtsError::AudioProcessingError("Нет аудиофрагментов для склейки".to_string()));
        }
        let sample_rate = audio_fragments[0].1;
        let mut final_audio = Vec::new();
        for (frag, _) in audio_fragments.iter() {
            final_audio.extend_from_slice(frag);
        }

        // 5. Нормализация громкости.
        // Если указан путь к исходному аудио, анализируем его уровень и приводим итоговое аудио к такому же уровню.
        let using_original = config.original_audio_path.is_some();
        send_progress(&config.progress_sender, ProgressUpdate::Normalizing { using_original }).await;
        if let Some(orig_path) = config.original_audio_path {
            // Декодируем исходное аудио с помощью symphonia.
            let (orig_samples, _) = audio::decode_audio_file(orig_path)
                .map_err(|e| TtsError::AudioProcessingError(format!("Ошибка декодирования исходного аудио для нормализации: {}", e)))?;
            
            let orig_rms = audio::compute_rms(&orig_samples);
            let final_rms = audio::compute_rms(&final_audio);
            
            if final_rms > 0.0 {
                let norm_factor = orig_rms / final_rms;
                for s in final_audio.iter_mut() {
                    *s *= norm_factor;
                }
            }
        } else {
            // Если исходное аудио не указано, нормализуем к заданному целевому уровню
            let max_amp = final_audio.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
            if max_amp > 0.0 {
                let norm_factor = config.audio_config.target_peak_level / max_amp;
                for s in final_audio.iter_mut() {
                    *s *= norm_factor;
                }
            }
        }

        // 6. Кодирование финального аудио в WAV.
        send_progress(&config.progress_sender, ProgressUpdate::Encoding).await;
        audio::encode_wav(&final_audio, sample_rate, config.output_wav.to_str().unwrap())?;

        send_progress(&config.progress_sender, ProgressUpdate::Finished).await;
        println!(
            "Итоговой аудиофайл записан: {}",
            config.output_wav.display()
        );
        Ok(())
    }
}
