// lib.rs

//! # TTS Audio Synchronizer Library
//!
//! Эта библиотека выполняет следующие задачи:
//! 1. Парсинг VTT-субтитров для получения таймингов и текста.
//! 2. Генерация аудиофрагментов через OpenAI TTS API (с параметризируемой конфигурацией).
//! 3. Декодирование аудио в PCM (f32) с помощью ffmpeg и hound.
//! 4. Адаптивная корректировка длительности фрагментов с помощью SoundTouch:
//!    - Интеллектуальное изменение темпа с сохранением высоты тона
//!    - Разное сжатие для речи и тишины
//!    - Специальная обработка для экстремального сжатия (>3x)
//! 5. Точная синхронизация с контрольными точками для минимизации накопления ошибок.
//! 6. Нормализация громкости:
//!    - По исходному аудио (если указан путь к mp3/m4a)
//!    - Стандартная нормализация к целевому уровню (если исходное аудио не указано)
//! 7. Сохранение промежуточных результатов для отладки:
//!    - MP3 фрагменты от TTS API
//!    - WAV файлы после коррекции длительности
//!    - Информация о каждом фрагменте
//!    - Копии финального аудио на разных этапах
//! 8. Асинхронная передача детальных обновлений о прогрессе выполнения.
//!
//! **Замечание:** Библиотека требует установленного SoundTouch и ffmpeg для работы.

use std::path::Path;
use std::process::Command;
use log::error;
use tokio::sync::mpsc::Sender;
use rubato::{SincFixedIn, FftFixedIn};
use anyhow::Context;

/// Модуль для работы с библиотекой SoundTouch через FFI
pub mod soundtouch {
    use super::TtsError;
    use super::Result;
    use log::{info, warn, error};
    use std::process::Command;
    use std::path::Path;
    use anyhow::Context;

    /// Структура для FFI-обертки SoundTouch
    #[repr(C)]
    pub struct SoundTouch {
        _private: [u8; 0],
    }

    /// FFI-обёртки для библиотеки SoundTouch.
    unsafe extern "C" {
        pub fn soundtouch_createInstance() -> *mut SoundTouch;
        pub fn soundtouch_destroyInstance(instance: *mut SoundTouch);
        pub fn soundtouch_setSampleRate(instance: *mut SoundTouch, srate: u32);
        pub fn soundtouch_setChannels(instance: *mut SoundTouch, numChannels: u32);
        pub fn soundtouch_setTempo(instance: *mut SoundTouch, newTempo: f32);
        pub fn soundtouch_setPitch(instance: *mut SoundTouch, newPitch: f32);
        pub fn soundtouch_putSamples(instance: *mut SoundTouch, samples: *const f32, numSamples: u32);
        pub fn soundtouch_receiveSamples(instance: *mut SoundTouch, outBuffer: *mut f32, maxSamples: u32) -> u32;
    }

    /// Проверяет, установлена ли библиотека SoundTouch
    pub fn is_soundtouch_installed() -> bool {
        #[cfg(target_os = "macos")]
        {
            // На macOS проверяем наличие библиотеки через Homebrew
            let output = Command::new("brew")
                .args(&["list", "sound-touch"])
                .output();
            
            match output {
                Ok(out) => out.status.success(),
                Err(_) => false,
            }
        }
        
        #[cfg(target_os = "linux")]
        {
            // На Linux проверяем наличие библиотеки через pkg-config или в стандартных путях
            let pkg_config = Command::new("pkg-config")
                .args(&["--exists", "soundtouch"])
                .status();
                
            match pkg_config {
                Ok(status) => status.success(),
                Err(_) => {
                    // Проверим наличие файла библиотеки в стандартных путях
                    Path::new("/usr/lib/libSoundTouch.so").exists() || 
                    Path::new("/usr/local/lib/libSoundTouch.so").exists()
                },
            }
        }
        
        #[cfg(target_os = "windows")]
        {
            // На Windows проверяем наличие DLL
            Path::new("C:\\Program Files\\SoundTouch\\bin\\SoundTouch.dll").exists() ||
            Path::new("C:\\Program Files (x86)\\SoundTouch\\bin\\SoundTouch.dll").exists()
        }
        
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            false // На других ОС просто возвращаем false
        }
    }

    /// Устанавливает библиотеку SoundTouch
    pub fn install_soundtouch() -> Result<()> {
        info!("Установка библиотеки SoundTouch...");
        
        #[cfg(target_os = "macos")]
        {
            let status = Command::new("brew")
                .args(&["install", "sound-touch"])
                .status()
                .map_err(|e| TtsError::Other(anyhow::anyhow!("Ошибка установки SoundTouch через Homebrew: {}", e)))?;
                
            if !status.success() {
                return Err(TtsError::Other(anyhow::anyhow!("Не удалось установить SoundTouch через Homebrew")));
            }
        }
        
        #[cfg(target_os = "linux")]
        {
            // Пробуем установить через apt для Debian/Ubuntu
            let apt_status = Command::new("apt-get")
                .args(&["install", "-y", "libsoundtouch-dev"])
                .status();
                
            if let Ok(status) = apt_status {
                if status.success() {
                    return Ok(());
                }
            }
            
            // Пробуем через pacman для Arch Linux
            let pacman_status = Command::new("pacman")
                .args(&["-S", "--noconfirm", "soundtouch"])
                .status();
                
            if let Ok(status) = pacman_status {
                if status.success() {
                    return Ok(());
                }
            }
            
            // Если ни один менеджер пакетов не сработал, возвращаем ошибку
            return Err(TtsError::Other(anyhow::anyhow!("Не удалось установить SoundTouch. Пожалуйста, установите вручную libsoundtouch-dev или аналогичный пакет для вашего дистрибутива")));
        }
        
        #[cfg(target_os = "windows")]
        {
            error!("Автоматическая установка SoundTouch на Windows не поддерживается");
            return Err(TtsError::Other(anyhow::anyhow!("Пожалуйста, скачайте и установите SoundTouch вручную с официального сайта")));
        }
        
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            return Err(TtsError::Other(anyhow::anyhow!("Автоматическая установка SoundTouch не поддерживается для данной ОС")));
        }
        
        info!("SoundTouch успешно установлен");
        Ok(())
    }

    /// Проверяет, установлен ли SoundTouch, и устанавливает его при необходимости
    pub fn ensure_soundtouch_installed() -> Result<()> {
        if !is_soundtouch_installed() {
            info!("SoundTouch не установлен, начинаем установку...");
            install_soundtouch()?;
        } else {
            info!("SoundTouch уже установлен");
        }
        Ok(())
    }

    /// Обёртка для обработки аудио через SoundTouch с сохранением pitch.
    pub fn process_with_soundtouch(input: &[f32], sample_rate: u32, tempo: f32) -> Result<Vec<f32>> {
        // Проверка установки SoundTouch теперь не нужна здесь, так как она выполняется
        // в начале всего процесса TTS в synchronizer::process_sync

        unsafe {
            let instance = soundtouch_createInstance();
            if instance.is_null() {
                return Err(TtsError::Other(anyhow::anyhow!("Не удалось создать экземпляр SoundTouch")));
            }
            
            // Используем RAII-паттерн для гарантированного уничтожения экземпляра
            struct SoundTouchInstance(*mut SoundTouch);
            impl Drop for SoundTouchInstance {
                fn drop(&mut self) {
                    unsafe { soundtouch_destroyInstance(self.0); }
                }
            }
            let _instance_guard = SoundTouchInstance(instance);
            
            soundtouch_setSampleRate(instance, sample_rate);
            soundtouch_setChannels(instance, 1);
            // Устанавливаем темп (tempo factor) — изменение длительности без изменения pitch.
            soundtouch_setTempo(instance, tempo);
            // Гарантируем, что тон остаётся неизменным.
            soundtouch_setPitch(instance, 1.0);
            // Передаём сэмплы.
            soundtouch_putSamples(instance, input.as_ptr(), input.len() as u32);

            // Считываем обработанные сэмплы.
            let mut output = Vec::new();
            let mut buffer = vec![0f32; 1024];
            loop {
                let received = soundtouch_receiveSamples(instance, buffer.as_mut_ptr(), buffer.len() as u32);
                if received == 0 {
                    break;
                }
                output.extend_from_slice(&buffer[..received as usize]);
            }
            
            Ok(output)
        }
    }
}

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
            window_size: 512,
            hop_size: 256,
            target_peak_level: 0.8,
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
    use log::{debug, info, warn, error};

    /// Генерирует аудиофрагмент через TTS API для заданного текста.
    /// Возвращает Vec<u8> с данными аудио (например, MP3) и текст для отладки.
    pub async fn generate_tts(api_key: &str, text: &str, config: &TtsConfig) -> Result<(Vec<u8>, String)> {
        let payload = json!({
            "model": config.model,
            "voice": config.voice,
            "input": text,
            "response_format": "mp3",
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
            
        info!("Получено {} байт аудио от OpenAI для текста: {}", audio_bytes.len(), text);
        
        if audio_bytes.is_empty() {
            warn!("Получен пустой ответ от OpenAI TTS API для текста: {}", text);
            return Err(TtsError::OpenAiApiError("Получен пустой ответ от API".to_string()));
        }
        
        // Проверяем, что первые байты похожи на MP3 (ID3 или MPEG заголовок)
        if audio_bytes.len() > 2 {
            let is_id3 = audio_bytes.len() > 3 && &audio_bytes[0..3] == b"ID3";
            let is_mpeg = audio_bytes.len() > 2 && (audio_bytes[0] == 0xFF && (audio_bytes[1] & 0xE0) == 0xE0);
            
            if !is_id3 && !is_mpeg {
                warn!("Получены данные, не похожие на MP3 (нет ID3/MPEG заголовка) для текста: {}", text);
            }
        }
        
        Ok((audio_bytes.to_vec(), text.to_string()))
    }
}

/// Модуль для аудио-обработки: декодирование, time-stretching, анализ громкости и кодирование.
pub mod audio {
    use super::{Result, TtsError, AudioProcessingConfig};
    use rubato::{SincFixedIn, FftFixedIn, Resampler};
    use log::{info, warn, error, debug};
    use std::path::Path;
    use std::process::Command;
    use hound;
    use tempfile;

    /// Декодирует MP3-данные из Vec<u8> в вектор f32-сэмплов (моно).
    /// Возвращает сэмплы и частоту дискретизации.
    pub fn decode_mp3(data: &[u8]) -> Result<(Vec<f32>, u32)> {
        // Проверяем, что переданные данные не пустые и имеют минимально допустимый размер
        if data.is_empty() {
            return Err(TtsError::AudioProcessingError("Получены пустые MP3 данные".to_string()));
        }
        
        if data.len() < 128 { // Минимальный размер для MP3 заголовка и фрейма
            return Err(TtsError::AudioProcessingError(
                format!("MP3 данные слишком малы для декодирования: {} байт", data.len())
            ));
        }
        
        // Создаем временный файл для MP3-данных
        let mut temp_file = tempfile::NamedTempFile::new()
            .map_err(|e| TtsError::IoError(e))?;
        
        // Записываем MP3 данные во временный файл
        std::io::Write::write_all(&mut temp_file, data)
            .map_err(|e| TtsError::IoError(e))?;
        
        // Получаем путь к временному файлу
        let temp_path = temp_file.path();
        
        // Вызываем нашу существующую функцию для декодирования аудиофайла
        let result = decode_audio_file(temp_path);
        
        // Дополнительная проверка результата декодирования
        match &result {
            Ok((samples, sample_rate)) => {
                if samples.is_empty() {
                    return Err(TtsError::AudioProcessingError("Декодировано 0 сэмплов из MP3 данных".to_string()));
                }
                
                debug!("Успешно декодировано {} сэмплов с частотой {} Гц из MP3 данных размером {} байт", 
                       samples.len(), sample_rate, data.len());
            },
            Err(e) => {
                debug!("Ошибка декодирования MP3 данных размером {} байт: {}", data.len(), e);
            }
        }
        
        result
    }

    /// Декодирует аудиофайл в PCM сэмплы
    pub fn decode_audio_file<P: AsRef<Path>>(path: P) -> Result<(Vec<f32>, u32)> {
        decode_audio_file_with_ffmpeg(path)
    }

    /// Декодирует аудиофайл с помощью ffmpeg
    pub fn decode_audio_file_with_ffmpeg<P: AsRef<Path>>(path: P) -> Result<(Vec<f32>, u32)> {
        debug!("Декодирование аудиофайла с помощью ffmpeg: {}", path.as_ref().display());
        
        // Создаем временный файл для WAV
        let temp_wav = tempfile::Builder::new()
            .suffix(".wav")
            .tempfile()
            .map_err(|e| TtsError::IoError(e))?;
        let temp_wav_path = temp_wav.path().to_str()
            .ok_or_else(|| TtsError::AudioProcessingError("Не удалось получить путь к временному файлу".to_string()))?;
        
        // Конвертируем аудио в WAV с помощью ffmpeg с улучшенными параметрами
        let output = Command::new("ffmpeg")
            .args(&[
                "-v", "warning",          // Уровень логирования
                "-stats",                 // Показывать прогресс
                "-i", path.as_ref().to_str().unwrap_or(""),
                "-ac", "1",                // Моно
                "-ar", "44100",           // 44.1 кГц
                "-sample_fmt", "s16",     // 16-bit PCM
                "-af", "aresample=resampler=soxr:precision=28:osf=s16", // Высококачественный ресемплер
                "-y",                     // Перезаписывать файлы без вопросов
                "-f", "wav",
                temp_wav_path
            ])
            .output()
            .map_err(|e| TtsError::AudioProcessingError(format!("Ошибка запуска ffmpeg: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Ошибка ffmpeg при декодировании {}: {}", path.as_ref().display(), stderr);
            return Err(TtsError::AudioProcessingError(format!("Ошибка ffmpeg: {}", stderr)));
        }
        
        // Проверяем размер полученного WAV файла
        let metadata = std::fs::metadata(temp_wav_path)
            .map_err(|e| TtsError::IoError(e))?;
        
        if metadata.len() < 44 {
            error!("Слишком маленький WAV файл после декодирования {} (размер: {} байт)", path.as_ref().display(), metadata.len());
            return Err(TtsError::AudioProcessingError(
                format!("Декодирование не удалось: полученный WAV файл слишком мал (размер: {} байт)", metadata.len())
            ));
        }

        // Читаем WAV-файл с помощью hound
        let reader = match hound::WavReader::open(temp_wav_path) {
            Ok(r) => r,
            Err(e) => {
                error!("Ошибка чтения WAV файла после декодирования {}: {}", path.as_ref().display(), e);
                return Err(TtsError::WavDecodingError(e));
            }
        };
        
        let spec = reader.spec();
        let sample_rate = spec.sample_rate;
        
        let samples_result = if spec.sample_format == hound::SampleFormat::Int {
            match spec.bits_per_sample {
                16 => reader.into_samples::<i16>()
                    .map(|s| s.map(|v| v as f32 / i16::MAX as f32))
                    .collect::<std::result::Result<Vec<_>, _>>(),
                24 => reader.into_samples::<i32>()
                    .map(|s| s.map(|v| v as f32 / (1 << 23) as f32))
                    .collect::<std::result::Result<Vec<_>, _>>(),
                32 => reader.into_samples::<i32>()
                    .map(|s| s.map(|v| v as f32 / i32::MAX as f32))
                    .collect::<std::result::Result<Vec<_>, _>>(),
                _ => return Err(TtsError::AudioProcessingError(format!("Неподдерживаемая битность: {}", spec.bits_per_sample)))
            }
        } else {
            reader.into_samples::<f32>()
                .collect::<std::result::Result<Vec<_>, _>>()
        };
        
        let samples = match samples_result {
            Ok(s) => {
                if s.is_empty() {
                    error!("Пустой WAV файл после декодирования {}", path.as_ref().display());
                    return Err(TtsError::AudioProcessingError("Декодирование не удалось: получен пустой WAV-файл".to_string()));
                }
                s
            },
            Err(e) => {
                error!("Ошибка чтения сэмплов из WAV файла после декодирования {}: {}", path.as_ref().display(), e);
                return Err(TtsError::WavDecodingError(e))
            }
        };
        
        debug!("Декодировано {} сэмплов с частотой {} Гц с помощью ffmpeg из {}", 
               samples.len(), sample_rate, path.as_ref().display());
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

    /// Detects speech vs. silence regions in audio samples
    fn detect_speech_regions(samples: &[f32], sample_rate: u32, silence_threshold: f32) -> Vec<(usize, usize, bool)> {
        let window_size = (sample_rate as f32 * 0.02).round() as usize; // 20ms window
        let mut regions = Vec::new();
        let mut is_speech = false;
        let mut region_start = 0;
        
        // Analyze audio in small windows
        for i in (0..samples.len()).step_by(window_size) {
            let end = (i + window_size).min(samples.len());
            let window = &samples[i..end];
            
            // Calculate RMS energy in this window
            let rms = compute_rms(window);
            let is_current_speech = rms > silence_threshold;
            
            // State change detection
            if is_current_speech != is_speech {
                // End previous region
                if i > 0 {
                    regions.push((region_start, i, is_speech));
                }
                // Start new region
                region_start = i;
                is_speech = is_current_speech;
            }
        }
        
        // Add final region if needed
        if region_start < samples.len() {
            regions.push((region_start, samples.len(), is_speech));
        }
        
        debug!("Detected {} speech/silence regions", regions.len());
        regions
    }

    /// Applies advanced time-stretching with non-uniform distribution
    /// Stretches silence regions more aggressively than speech regions
    pub fn adaptive_time_stretch(
        input: &[f32], 
        actual_duration: f32, 
        target_duration: f32, 
        sample_rate: u32,
        config: &AudioProcessingConfig
    ) -> Result<Vec<f32>> {
        info!("Applying adaptive time-stretching: actual={:.3}s, target={:.3}s", 
              actual_duration, target_duration);
        
        if actual_duration <= target_duration {
            let target_samples = (target_duration * sample_rate as f32).round() as usize;
            let mut output = input.to_vec();
            if output.len() < target_samples {
                let silence_samples = target_samples - output.len();
                let prefix_silence = silence_samples / 3;
                let suffix_silence = silence_samples - prefix_silence;
                
                let mut new_output = vec![0.0; target_samples];
                new_output[prefix_silence..prefix_silence + output.len()].copy_from_slice(&output);
                output = new_output;
            }
            return Ok(output);
        }

        // Detect speech/silence regions with original threshold
        let silence_threshold = 0.01; // Restored original threshold
        let regions = detect_speech_regions(input, sample_rate, silence_threshold);
        
        if regions.is_empty() {
            warn!("No speech/silence regions detected, falling back to uniform time-stretching");
            return super::soundtouch::process_with_soundtouch(
                input, 
                sample_rate, 
                actual_duration / target_duration
            );
        }
        
        // Calculate how much total time we need to remove
        let time_to_remove = actual_duration - target_duration;
        
        // Calculate total duration of silence vs speech
        let mut silence_duration = 0.0;
        let mut speech_duration = 0.0;
        
        for (start, end, is_speech) in &regions {
            let region_duration = (*end - *start) as f32 / sample_rate as f32;
            if *is_speech {
                speech_duration += region_duration;
            } else {
                silence_duration += region_duration;
            }
        }
        
        // Determine stretch factors using original logic
        let (silence_factor, speech_factor) = if silence_duration > 0.0 {
            // Try to remove most time from silence, but not more than 80% of silence
            let max_silence_reduction = silence_duration * 0.8;
            
            if max_silence_reduction >= time_to_remove {
                // We can remove all needed time from silence
                let silence_compression = silence_duration / (silence_duration - time_to_remove);
                (silence_compression, 1.0) // Don't compress speech at all
            } else {
                // Remove what we can from silence, and the rest from speech
                let remaining_time = time_to_remove - max_silence_reduction;
                let silence_compression = 3.0; // Original max silence compression
                let speech_compression = speech_duration / (speech_duration - remaining_time);
                (silence_compression, speech_compression)
            }
        } else {
            // No silence, compress speech uniformly
            (1.0, actual_duration / target_duration)
        };
        
        info!("Adaptive factors: silence={:.2}, speech={:.2}", silence_factor, speech_factor);
        
        // Apply stretching to regions
        let mut output = Vec::new();
        
        for (start, end, is_speech) in regions {
            let region = &input[start..end];
            let factor = if is_speech { speech_factor } else { silence_factor };
            
            // Skip processing for regions with factor close to 1.0
            if (factor - 1.0).abs() < 0.05 {
                output.extend_from_slice(region);
                continue;
            }
            
            let processed = super::soundtouch::process_with_soundtouch(
                region, 
                sample_rate, 
                factor
            )?;
            
            output.extend(processed);
        }

        // Final length adjustment if needed
        let target_samples = (target_duration * sample_rate as f32).round() as usize;
        if output.len() > target_samples {
            output.truncate(target_samples);
        } else if output.len() < target_samples {
            output.extend(vec![0.0; target_samples - output.len()]);
        }
        
        Ok(output)
    }

    /// Вычисляет RMS-уровень (корень из среднего квадрата) для набора сэмплов.
    pub fn compute_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }

    /// Applies time-stretching to audio for duration adjustment with improved algorithm.
    pub fn adjust_duration(
        input: &[f32],
        actual_duration: f32,
        target_duration: f32,
        sample_rate: u32,
        config: &AudioProcessingConfig,
    ) -> Result<Vec<f32>> {
        if input.is_empty() {
            warn!("adjust_duration received empty input audio!");
            return Err(TtsError::AudioProcessingError("Attempt to process empty audio".to_string()));
        }

        if target_duration <= 0.0 {
            warn!("Invalid target duration: {:.3}s, using actual duration instead", target_duration);
            return Ok(input.to_vec());
        }

        info!("Applying time-stretching: input audio {} samples, actual_duration={:.3}s, target_duration={:.3}s, factor={:.3}", 
              input.len(), actual_duration, target_duration, actual_duration / target_duration);
        
        // Check if audio is silence
        let max_amplitude = input.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        if max_amplitude < 0.001 {
            warn!("Input audio has very low amplitude: {:.6}, possibly silence. Just adding silence.", max_amplitude);
            let target_samples = (target_duration * sample_rate as f32).round() as usize;
            let mut output = input.to_vec();
            if output.len() < target_samples {
                output.extend(vec![0.0; target_samples - output.len()]);
            } else {
                output.truncate(target_samples);
            }
            return Ok(output);
        }

        // Special handling for very short phrases (less than 1 second or few words)
        let is_short_phrase = actual_duration < 1.0;
        if is_short_phrase {
            info!("Short phrase detected ({:.3}s), using minimal time adjustment", actual_duration);
            
            if actual_duration > target_duration {
                // For compression, use very mild stretching to preserve speech quality
                let max_compression = 1.2; // Limit compression for short phrases
                let stretch_factor = (actual_duration / target_duration).min(max_compression);
                
                if stretch_factor > 1.05 {
                    // Only apply stretching if factor is significant
                    let processed = super::soundtouch::process_with_soundtouch(
                        input, 
                        sample_rate,
                        stretch_factor
                    )?;
                    
                    // If still too long, trim the end gradually
                    let target_samples = (target_duration * sample_rate as f32).round() as usize;
                    if processed.len() > target_samples {
                        let mut output = processed;
                        // Apply fade out to last 10ms
                        let fade_samples = (0.01 * sample_rate as f32) as usize;
                        let fade_start = target_samples.saturating_sub(fade_samples);
                        for i in fade_start..target_samples {
                            let factor = (target_samples - i) as f32 / fade_samples as f32;
                            if i < output.len() {
                                output[i] *= factor;
                            }
                        }
                        output.truncate(target_samples);
                        return Ok(output);
                    }
                    return Ok(processed);
                }
            }
            
            // For extension or minimal compression, just add silence
            let target_samples = (target_duration * sample_rate as f32).round() as usize;
            let mut output = input.to_vec();
            
            if output.len() != target_samples {
                let mut new_output = vec![0.0; target_samples];
                // Center the audio with slightly more silence at the end
                let prefix_silence = ((target_samples - output.len()) as f32 * 0.4) as usize;
                new_output[prefix_silence..prefix_silence + output.len()].copy_from_slice(&output);
                return Ok(new_output);
            }
            
            return Ok(output);
        }
        
        // Safe version - just add silence without changing tempo if audio is too short
        if input.len() < 100 || actual_duration < 0.1 || target_duration < 0.1 {
            warn!("Audio too short for time-stretching: {} samples, {:.3}s -> {:.3}s, adding silence", 
                  input.len(), actual_duration, target_duration);
            let target_samples = (target_duration * sample_rate as f32).round() as usize;
            let mut output = input.to_vec();
            if output.len() < target_samples {
                output.extend(vec![0.0; target_samples - output.len()]);
            } else if output.len() > target_samples {
                output.truncate(target_samples);
            }
            return Ok(output);
        }

        if actual_duration > target_duration {
            // Case 1: Audio needs to be sped up (using adaptive algorithm)
            adaptive_time_stretch(input, actual_duration, target_duration, sample_rate, config)
        } else {
            // Case 2: Audio is shorter than target - add silence more intelligently
            let target_samples = (target_duration * sample_rate as f32).round() as usize;
            let mut output = input.to_vec();
            
            if output.len() < target_samples {
                // Add 1/3 of the silence at the beginning and 2/3 at the end for more natural timing
                let silence_samples = target_samples - output.len();
                let prefix_silence = silence_samples / 3;
                let suffix_silence = silence_samples - prefix_silence;
                
                let mut new_output = vec![0.0; target_samples];
                new_output[prefix_silence..prefix_silence + output.len()].copy_from_slice(&output);
                
                return Ok(new_output);
            }
            
            Ok(output)
        }
    }
}

/// Основной API библиотеки.
pub mod synchronizer {
    use super::{audio, tts, vtt, ProgressUpdate, Result, TtsError, TtsConfig, AudioProcessingConfig};
    use futures::future::join_all;
    use std::path::Path;
    use tokio::sync::mpsc::Sender;
    use log::{debug, info, error, warn};

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

    /// Структура для отслеживания синхронизации
    #[derive(Debug)]
    struct SyncState {
        expected_duration: f32,
        actual_duration: f32,
        cumulative_error: f32,
        last_checkpoint: f32,
    }

    impl SyncState {
        fn new() -> Self {
            Self {
                expected_duration: 0.0,
                actual_duration: 0.0,
                cumulative_error: 0.0,
                last_checkpoint: 0.0,
            }
        }

        fn update(&mut self, expected: f32, actual: f32) {
            self.expected_duration += expected;
            self.actual_duration += actual;
            self.cumulative_error = self.actual_duration - self.expected_duration;
        }

        fn needs_correction(&self) -> bool {
            self.cumulative_error.abs() > 0.1 // Коррекция при ошибке более 100мс
        }

        fn get_correction_factor(&self) -> f32 {
            if self.needs_correction() {
                // Вычисляем фактор коррекции для следующего сегмента
                let remaining_expected = self.expected_duration - self.last_checkpoint;
                if remaining_expected > 0.0 {
                    (remaining_expected - self.cumulative_error) / remaining_expected
                } else {
                    1.0
                }
            } else {
                1.0
            }
        }

        fn checkpoint(&mut self) {
            self.last_checkpoint = self.expected_duration;
            // Сбрасываем накопленную ошибку при достижении контрольной точки
            if self.cumulative_error.abs() < 0.05 {
                self.cumulative_error = 0.0;
            }
        }
    }

    /// Выполняет полный процесс синхронизации:
    /// - Парсинг VTT
    /// - Генерация аудио через TTS API
    /// - Декодирование, корректировка длительности, применение fade‑in/fade‑out для каждого аудиофрагмента
    /// - Склейка фрагментов, нормализация громкости (если указан оригинальный аудиофайл), запись итогового аудио в WAV.
    pub async fn process_sync(config: SyncConfig<'_>) -> Result<()> {
        send_progress(&config.progress_sender, ProgressUpdate::Started).await;

        // Сначала проверяем, установлен ли SoundTouch, и устанавливаем его при необходимости
        info!("Проверка установки SoundTouch перед началом TTS обработки");
        match super::soundtouch::ensure_soundtouch_installed() {
            Ok(_) => info!("SoundTouch доступен, приступаем к TTS обработке"),
            Err(e) => {
                error!("Не удалось обеспечить наличие SoundTouch: {}", e);
                return Err(e);
            }
        }

        // 1. Парсинг VTT
        send_progress(&config.progress_sender, ProgressUpdate::ParsingVTT).await;
        let cues = vtt::parse_vtt(&config.vtt_path)?;
        send_progress(&config.progress_sender, ProgressUpdate::ParsedVTT { total: cues.len() }).await;
        println!("Найдено {} реплик", cues.len());

        // Создаем директорию для сохранения MP3-чанков
        let debug_dir = config.output_wav.parent()
            .ok_or_else(|| TtsError::ConfigError("Некорректный путь к выходному файлу".to_string()))?
            .join("debug_mp3_chunks");
        
        if !debug_dir.exists() {
            std::fs::create_dir_all(&debug_dir)
                .map_err(|e| TtsError::IoError(e))?;
            info!("Создана директория для отладочных MP3-файлов: {}", debug_dir.display());
        }

        // 2. Генерация TTS для каждой реплики параллельно
        let tts_futures = cues.iter().enumerate().filter_map(|(i, cue)| {
            // Пропускаем пустые сегменты
            if cue.text.trim().is_empty() {
                info!("Пропускаем пустой сегмент #{} ({}s -> {}s)", i, cue.start, cue.end);
                None
            } else {
                let api_key = config.api_key;
                let text = cue.text.clone();
                let tts_config = &config.tts_config;
                Some(async move {
                    let res = tts::generate_tts(api_key, &text, tts_config).await;
                    (i, res)
                })
            }
        });
        let tts_results = join_all(tts_futures).await;
        let mut audio_fragments = Vec::new();

        let mut sync_state = SyncState::new();
        let checkpoint_interval = 10.0; // Контрольная точка каждые 10 секунд
        let mut next_checkpoint = checkpoint_interval;
        let mut current_time = 0.0;

        // 3. Обработка каждого аудиофрагмента с учетом синхронизации
        let mut prev_end: Option<f32> = None;
        for (i, cue) in cues.iter().enumerate() {
            send_progress(&config.progress_sender, ProgressUpdate::TTSGeneration { current: i + 1, total: cues.len() }).await;
            
            // Проверяем стык с предыдущим сегментом
            if let Some(prev_end_time) = prev_end {
                let gap = cue.start - prev_end_time;
                if gap.abs() < 0.001 { // Если разница меньше 1мс, считаем что сегменты смежные
                    info!("Обнаружены смежные сегменты на {:.3}s", cue.start);
                } else if gap < 0.0 {
                    warn!("Обнаружено наложение сегментов на {:.3}s: overlap={:.3}s", cue.start, -gap);
                } else if gap > 0.0 {
                    info!("Обнаружен разрыв между сегментами на {:.3}s: gap={:.3}s", cue.start, gap);
                }
            }
            prev_end = Some(cue.end);
            
            // Для пустых сегментов создаем тишину
            if cue.text.trim().is_empty() {
                let target_duration = cue.end - cue.start;
                if target_duration <= 0.0 {
                    warn!("Пропускаем сегмент с некорректной длительностью: start={:.3}s, end={:.3}s", cue.start, cue.end);
                    continue;
                }
                let silence_samples = (target_duration * 44100.0) as usize;
                let silence = vec![0.0; silence_samples];
                audio_fragments.push((silence, 44100, String::from("[silence]")));
                continue;
            }
            
            // Ищем результат TTS для этого сегмента
            let tts_result = tts_results.iter().find(|(idx, _)| *idx == i);
            
            if let Some((_, result)) = tts_result {
                let (audio_bytes, text) = match result {
                    Ok(res) => res,
                    Err(e) => {
                        error!("Ошибка генерации TTS для сегмента #{}: {}", i, e);
                        return Err(TtsError::OpenAiApiError(e.to_string()));
                    }
                };
                
                // Получаем имя видео из пути к выходному файлу
                let base_filename = config.output_wav
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output")
                    .to_string();
                
                // Формируем имя файла без текста субтитров - только номер чанка и имя видео
                let chunk_name = format!("chunk_{:03}_{}", i, base_filename);
                
                let chunk_path = debug_dir.join(format!("{}.mp3", chunk_name));
                std::fs::write(&chunk_path, &audio_bytes)
                    .map_err(|e| TtsError::IoError(e))?;
                
                info!("Сохранен MP3-чанк №{}: {} байт, путь: {}", i, audio_bytes.len(), chunk_path.display());
                
                // Проверяем размер аудио-чанка
                if audio_bytes.len() < 100 {
                    warn!("Слишком маленький размер MP3-чанка №{}: {} байт. Текст: {}", i, audio_bytes.len(), text);
                    // Создаем файл с ошибкой и продолжаем со следующим фрагментом
                    let error_path = debug_dir.join(format!("{}_ERROR_TOO_SMALL.txt", chunk_name));
                    let error_info = format!("Слишком маленький размер MP3: {} байт\nТекст: {}", audio_bytes.len(), text);
                    std::fs::write(error_path, error_info)
                        .map_err(|e| TtsError::IoError(e))?;
                    continue;
                }
                
                // Продолжаем обычную обработку
                let decode_result = audio::decode_mp3(&audio_bytes);
                let (pcm, sample_rate) = match decode_result {
                    Ok(result) => result,
                    Err(e) => {
                        error!("Ошибка декодирования MP3-чанка №{}: {}. Текст: {}", i, e, text);
                        continue;
                    }
                };
                
                let actual_duration = audio::duration_in_seconds(pcm.len(), sample_rate);
                let target_duration = cue.end - cue.start;
                
                // Обновляем состояние синхронизации
                sync_state.update(target_duration, actual_duration);
                
                // Проверяем необходимость контрольной точки
                if current_time >= next_checkpoint {
                    info!("Контрольная точка синхронизации на {:.2}s: ошибка={:.3}s", 
                          current_time, sync_state.cumulative_error);
                    sync_state.checkpoint();
                    next_checkpoint = current_time + checkpoint_interval;
                }
                
                // Получаем фактор коррекции для текущего сегмента
                let correction_factor = sync_state.get_correction_factor();
                
                // Применяем коррекцию к целевой длительности
                let adjusted_target = if correction_factor != 1.0 {
                    info!("Применяем коррекцию синхронизации для чанка №{}: фактор={:.3}", i, correction_factor);
                    target_duration * correction_factor
                } else {
                    target_duration
                };
                
                send_progress(
                    &config.progress_sender,
                    ProgressUpdate::ProcessingFragment {
                        index: i + 1,
                        total: cues.len(),
                        step: format!(
                            "Длительность: target={:.3}s (adjusted={:.3}s), actual={:.3}s, sync_error={:.3}s",
                            target_duration, adjusted_target, actual_duration, sync_state.cumulative_error
                        ),
                    },
                ).await;
                
                // Корректируем длительность с учетом синхронизации и смежных сегментов
                let adjusted = if i > 0 && (cue.start - cues[i-1].end).abs() < 0.001 {
                    // Для смежных сегментов применяем плавный кроссфейд
                    let crossfade_duration = 0.010; // 10мс
                    let crossfade_samples = (crossfade_duration * sample_rate as f32) as usize;
                    
                    let mut adjusted = audio::adjust_duration(&pcm, actual_duration, adjusted_target, sample_rate, &config.audio_config)?;
                    
                    // Применяем фейд к началу текущего сегмента
                    for i in 0..crossfade_samples.min(adjusted.len()) {
                        let factor = i as f32 / crossfade_samples as f32;
                        adjusted[i] *= factor;
                    }
                    
                    // Если есть предыдущий фрагмент, применяем к нему фейд
                    if let Some(last) = audio_fragments.last_mut() {
                        let last_samples = &mut last.0;
                        let fade_start = last_samples.len().saturating_sub(crossfade_samples);
                        for i in fade_start..last_samples.len() {
                            let factor = (last_samples.len() - i) as f32 / crossfade_samples as f32;
                            last_samples[i] *= factor;
                        }
                    }
                    
                    adjusted
                } else {
                    audio::adjust_duration(&pcm, actual_duration, adjusted_target, sample_rate, &config.audio_config)?
                };
                
                // Сохраняем WAV после коррекции длительности для отладки
                let adjusted_wav_path = debug_dir.join(format!("{}_adjusted.wav", chunk_name));
                if let Err(e) = audio::encode_wav(&adjusted, sample_rate, adjusted_wav_path.to_str().unwrap()) {
                    warn!("Не удалось сохранить скорректированный WAV для чанка №{}: {}", i, e);
                }
                
                // Обновляем текущее время
                current_time = cue.end;
                
                // Добавляем обработанный фрагмент
                audio_fragments.push((adjusted, sample_rate, text.to_string()));
            }
        }

        // Финальная проверка синхронизации
        if sync_state.needs_correction() {
            warn!(
                "Финальная ошибка синхронизации: {:.3}s (ожидалось={:.3}s, фактически={:.3}s)",
                sync_state.cumulative_error, sync_state.expected_duration, sync_state.actual_duration
            );
        } else {
            info!(
                "Синхронизация в пределах допуска: ошибка={:.3}s, длительность={:.3}s",
                sync_state.cumulative_error, sync_state.expected_duration
            );
        }

        // 4. Склейка аудиофрагментов
        send_progress(&config.progress_sender, ProgressUpdate::MergingFragments).await;
        if audio_fragments.is_empty() {
            return Err(TtsError::AudioProcessingError("Нет аудиофрагментов для склейки".to_string()));
        }
        let sample_rate = audio_fragments[0].1;
        let mut final_audio = Vec::new();
        
        // Создаем информационный файл о каждом фрагменте
        let fragments_info_path = debug_dir.join("fragments_info.txt");
        let mut fragments_info = String::new();
        fragments_info.push_str("Информация об аудиофрагментах:\n\n");
        
        for (i, (frag, sr, text)) in audio_fragments.iter().enumerate() {
            if *sr != sample_rate {
                warn!("Фрагмент №{} имеет другую частоту дискретизации: {} Гц (ожидалось {} Гц)", i, sr, sample_rate);
            }
            
            let frag_duration = audio::duration_in_seconds(frag.len(), *sr);
            let frag_info = format!("Фрагмент №{}: длительность={:.3}s, sample_rate={}Hz, samples={}, текст: {}\n", 
                                 i, frag_duration, sr, frag.len(), text);
            fragments_info.push_str(&frag_info);
            
            final_audio.extend_from_slice(frag);
        }
        
        std::fs::write(fragments_info_path, fragments_info)
            .map_err(|e| TtsError::IoError(e))?;

        // Сохраняем сырой склеенный аудиофайл перед нормализацией
        let merged_wav_path = debug_dir.join("merged_raw.wav");
        if let Err(e) = audio::encode_wav(&final_audio, sample_rate, merged_wav_path.to_str().unwrap()) {
            warn!("Не удалось сохранить сырой склеенный WAV: {}", e);
        }

        // 5. Нормализация громкости.
        // Если указан путь к исходному аудио, анализируем его уровень и приводим итоговое аудио к такому же уровню.
        let using_original = config.original_audio_path.is_some();
        send_progress(&config.progress_sender, ProgressUpdate::Normalizing { using_original }).await;
        
        let mut normalization_applied = false;
        
        if let Some(orig_path) = config.original_audio_path {
            // Декодируем исходное аудио с помощью улучшенной функции
            match audio::decode_audio_file(orig_path) {
                Ok((orig_samples, _)) => {
                    if orig_samples.is_empty() {
                        warn!("Исходное аудио не содержит сэмплов. Будет использована стандартная нормализация.");
                    } else {
                        let orig_rms = audio::compute_rms(&orig_samples);
                        let final_rms = audio::compute_rms(&final_audio);
                        
                        if final_rms > 0.0 && orig_rms > 0.0 {
                            let norm_factor = orig_rms / final_rms;
                            info!("Нормализация громкости: исходный RMS = {:.6}, итоговый RMS = {:.6}, коэффициент = {:.6}", 
                                orig_rms, final_rms, norm_factor);
                            for s in final_audio.iter_mut() {
                                *s *= norm_factor;
                            }
                            normalization_applied = true;
                            
                            // Сохраняем нормализованный аудиофайл
                            let norm_orig_wav_path = debug_dir.join("normalized_by_original.wav");
                            if let Err(e) = audio::encode_wav(&final_audio, sample_rate, norm_orig_wav_path.to_str().unwrap()) {
                                warn!("Не удалось сохранить нормализованный WAV (по оригиналу): {}", e);
                            }
                        } else {
                            warn!("Пропуск нормализации: исходный RMS = {:.6}, итоговый RMS = {:.6}", orig_rms, final_rms);
                        }
                    }
                },
                Err(e) => {
                    // Если не удалось декодировать исходное аудио, логируем ошибку и продолжаем без нормализации
                    error!("Не удалось декодировать исходное аудио для нормализации: {}. Будет использована стандартная нормализация.", e);
                }
            }
        }
        
        // Если нормализация по оригинальному аудио не была выполнена, 
        // используем стандартную нормализацию к целевому уровню
        if !normalization_applied {
            // Стандартная нормализация к заданному целевому уровню
            let max_amp = final_audio.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
            if max_amp > 0.0 {
                let norm_factor = config.audio_config.target_peak_level / max_amp;
                info!("Используем стандартную нормализацию: max_amp = {:.6}, коэффициент = {:.6}", max_amp, norm_factor);
                for s in final_audio.iter_mut() {
                    *s *= norm_factor;
                }
                normalization_applied = true;
                
                // Сохраняем нормализованный аудиофайл
                let norm_std_wav_path = debug_dir.join("normalized_standard.wav");
                if let Err(e) = audio::encode_wav(&final_audio, sample_rate, norm_std_wav_path.to_str().unwrap()) {
                    warn!("Не удалось сохранить нормализованный WAV (стандартный): {}", e);
                }
            } else {
                error!("Не удалось нормализовать аудио: финальное аудио не содержит ненулевых сэмплов!");
                return Err(TtsError::AudioProcessingError("Генерация TTS не удалась: получено пустое аудио".to_string()));
            }
        }

        // Финальная проверка аудио перед сохранением
        if final_audio.is_empty() {
            error!("Не удалось создать аудио: итоговое аудио пустое!");
            return Err(TtsError::AudioProcessingError("Генерация TTS не удалась: итоговое аудио пустое".to_string()));
        }
        
        let max_amp_final = final_audio.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        if max_amp_final <= 0.0001 {
            warn!("Итоговое аудио имеет очень низкую амплитуду: {:.6}. Возможно некорректная нормализация.", max_amp_final);
        }

        // Сохраняем финальное аудио перед кодированием для отладки
        let final_debug_wav_path = debug_dir.join("final_before_encoding.wav");
        if let Err(e) = audio::encode_wav(&final_audio, sample_rate, final_debug_wav_path.to_str().unwrap()) {
            warn!("Не удалось сохранить финальный WAV для отладки: {}", e);
        } else {
            info!("Сохранен финальный WAV для отладки: {}", final_debug_wav_path.display());
        }

        // 6. Кодирование финального аудио в WAV.
        send_progress(&config.progress_sender, ProgressUpdate::Encoding).await;
        info!("Кодирование финального аудио в WAV. Сэмплов: {}, частота: {} Гц, макс.амплитуда: {:.6}", 
              final_audio.len(), sample_rate, max_amp_final);
        
        match audio::encode_wav(&final_audio, sample_rate, config.output_wav.to_str().unwrap()) {
            Ok(_) => {
                info!("Успешно закодирован WAV-файл: {}", config.output_wav.display());
            },
            Err(e) => {
                error!("Ошибка при кодировании WAV-файла: {}", e);
                return Err(e);
            }
        }
        
        // Проверяем, что файл действительно создан и имеет ненулевой размер
        let output_metadata = match std::fs::metadata(config.output_wav) {
            Ok(meta) => meta,
            Err(e) => {
                error!("Не удалось получить информацию о созданном файле: {}", e);
                return Err(TtsError::IoError(e));
            }
        };
        
        if output_metadata.len() < 44 { // 44 байта - размер заголовка WAV
            error!("Не удалось создать аудиофайл: размер итогового файла слишком мал ({} байт)", output_metadata.len());
            return Err(TtsError::AudioProcessingError(format!(
                "Генерация TTS не удалась: итоговый файл слишком мал ({} байт)", output_metadata.len()
            )));
        }

        // Копируем финальный файл для отладки
        let final_copy_path = debug_dir.join("final_output_copy.wav");
        if let Err(e) = std::fs::copy(config.output_wav, &final_copy_path) {
            warn!("Не удалось создать копию итогового файла: {}", e);
        } else {
            info!("Создана копия итогового файла: {}", final_copy_path.display());
        }

        send_progress(&config.progress_sender, ProgressUpdate::Finished).await;
        println!(
            "Итоговой аудиофайл записан: {} (размер: {} байт)",
            config.output_wav.display(),
            output_metadata.len()
        );
        Ok(())
    }
}
