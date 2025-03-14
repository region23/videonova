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
            voice: "ash".to_string(),  // Всегда используем мужской голос
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
    /// Баланс между голосом и инструментальной дорожкой (0.0 - 1.0)
    /// 0.5 означает равный баланс
    pub voice_to_instrumental_ratio: f32,
    /// Коэффициент усиления инструментальной дорожки (1.0 = без изменений)
    pub instrumental_boost: f32,
}

impl Default for AudioProcessingConfig {
    fn default() -> Self {
        Self {
            window_size: 512,
            hop_size: 256,
            target_peak_level: 0.8,
            voice_to_instrumental_ratio: 0.4, // Баланс: 40% голос, 60% музыка
            instrumental_boost: 1.5, // Усиление инструментальной дорожки в 1.5 раза
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
                let text = lines.collect::<Vec<_>>().join(" ").trim().to_string();
                // Пропускаем пустые субтитры
                if !text.is_empty() {
                    cues.push(SubtitleCue { start, end, text });
                }
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
    use tokio::time::{sleep, Duration};

    /// Генерирует аудиофрагмент через TTS API для заданного текста.
    /// Возвращает Vec<u8> с данными аудио (например, MP3) и текст для отладки.
    pub async fn generate_tts(api_key: &str, text: &str, config: &TtsConfig) -> Result<(Vec<u8>, String)> {
        const MAX_RETRIES: u32 = 5;
        const INITIAL_BACKOFF_MS: u64 = 1000;

        let payload = json!({
            "model": config.model,
            "voice": config.voice,
            "input": text,
            "response_format": "mp3",
            "speed": config.speed
        });

        let client = Client::new();
        let mut attempt = 0;
        let mut last_error = None;

        while attempt < MAX_RETRIES {
            if attempt > 0 {
                let backoff = INITIAL_BACKOFF_MS * (2_u64.pow(attempt - 1));
                info!("Повторная попытка #{} через {} мс...", attempt + 1, backoff);
                sleep(Duration::from_millis(backoff)).await;
            }

            let resp = match client
                .post("https://api.openai.com/v1/audio/speech")
                .bearer_auth(api_key)
                .json(&payload)
                .send()
                .await {
                    Ok(r) => r,
                    Err(e) => {
                        warn!("Ошибка сети при попытке #{}: {}", attempt + 1, e);
                        last_error = Some(TtsError::HttpError(e));
                        attempt += 1;
                        continue;
                    }
                };

            if resp.status().is_success() {
                let audio_bytes = resp.bytes().await
                    .map_err(|e| TtsError::HttpError(e))?;
                    
                info!("Получено {} байт аудио от OpenAI для текста: {}", audio_bytes.len(), text);
                
                if audio_bytes.is_empty() {
                    warn!("Получен пустой ответ от OpenAI TTS API для текста: {}", text);
                    last_error = Some(TtsError::OpenAiApiError("Получен пустой ответ от API".to_string()));
                    attempt += 1;
                    continue;
                }
                
                // Проверяем, что первые байты похожи на MP3
                if audio_bytes.len() > 2 {
                    let is_id3 = audio_bytes.len() > 3 && &audio_bytes[0..3] == b"ID3";
                    let is_mpeg = audio_bytes.len() > 2 && (audio_bytes[0] == 0xFF && (audio_bytes[1] & 0xE0) == 0xE0);
                    
                    if !is_id3 && !is_mpeg {
                        warn!("Получены данные, не похожие на MP3 (нет ID3/MPEG заголовка) для текста: {}", text);
                    }
                }
                
                return Ok((audio_bytes.to_vec(), text.to_string()));
            } else {
                let status = resp.status();
                let error_text = resp.text().await.unwrap_or_else(|_| "Неизвестная ошибка".to_string());
                
                // Для 503 ошибок всегда делаем повторную попытку
                if status == 503 {
                    warn!("Сервер перегружен (503), попытка #{}: {}", attempt + 1, error_text);
                    last_error = Some(TtsError::OpenAiApiError(format!(
                        "Ошибка API (код {}): {}", status, error_text
                    )));
                    attempt += 1;
                    continue;
                }
                
                // Для других ошибок возвращаем сразу
                return Err(TtsError::OpenAiApiError(format!(
                    "Ошибка API (код {}): {}", status, error_text
                )));
            }
        }

        // Если все попытки исчерпаны, возвращаем последнюю ошибку
        Err(last_error.unwrap_or_else(|| TtsError::OpenAiApiError(
            "Превышено максимальное количество попыток".to_string()
        )))
    }
}

/// Модуль для работы с Demucs через командную строку
pub mod demucs {
    use super::{TtsError, Result};
    use log::{info, warn, error};
    use std::process::Command;
    use std::path::Path;
    use tokio::sync::mpsc::Sender;
    use serde_json::json;

    #[derive(Debug)]
    pub enum DemucsSeparationProgress {
        Started,
        LoadingModel,
        Processing { progress: f32 },
        Converting,
        Finished,
        Error(String),
    }

    /// Удаляет вокал из аудиофайла с помощью Demucs
    pub async fn remove_vocals<P: AsRef<Path>>(
        input_path: P,
        output_path: P,
        progress_sender: Option<Sender<DemucsSeparationProgress>>,
        debug_dir: Option<P>,
    ) -> Result<()> {
        // Проверяем установку Demucs
        ensure_demucs_installed().await?;

        info!("Удаление вокала с помощью Demucs из файла: {}", input_path.as_ref().display());
        
        // Отправляем статус начала работы
        send_progress(&progress_sender, DemucsSeparationProgress::Started).await;

        // Создаем временную директорию для результатов Demucs
        let temp_dir = tempfile::tempdir()
            .map_err(|e| TtsError::IoError(e))?;

        // Отправляем статус загрузки модели
        send_progress(&progress_sender, DemucsSeparationProgress::LoadingModel).await;

        // Создаем канал для передачи прогресса из потока чтения вывода
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(32);
        let progress_sender_clone = progress_sender.clone();

        // Запускаем Demucs с выводом прогресса
        let mut child = tokio::process::Command::new("demucs")
            .args(&[
                "--two-stems=vocals",  // Разделяем только на вокал и остальное
                "-n", "htdemucs",      // Используем лучшую модель
                "--mp3",               // Выходной формат MP3 для экономии места
                "-o", temp_dir.path().to_str().unwrap(),
                input_path.as_ref().to_str().unwrap(),
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| TtsError::AudioProcessingError(format!("Ошибка запуска Demucs: {}", e)))?;

        // Читаем вывод в реальном времени для отслеживания прогресса
        let stderr = child.stderr.take().unwrap();
        
        // Запускаем задачу для чтения вывода
        tokio::spawn(async move {
            use tokio::io::{BufReader, AsyncBufReadExt};
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            
            while let Ok(Some(line)) = lines.next_line().await {
                // Анализируем вывод Demucs для определения прогресса
                if let Some(progress) = parse_demucs_progress(&line) {
                    let _ = progress_tx.send(progress).await;
                }
                // Логируем все строки для отладки
                info!("Demucs output: {}", line);
            }
        });

        // Обрабатываем прогресс
        tokio::spawn(async move {
            while let Some(progress) = progress_rx.recv().await {
                send_progress(&progress_sender_clone, 
                    DemucsSeparationProgress::Processing { progress }).await;
            }
        });

        // Ждем завершения процесса
        let status = child.wait()
            .await
            .map_err(|e| TtsError::AudioProcessingError(format!("Ошибка выполнения Demucs: {}", e)))?;

        if !status.success() {
            let error_msg = format!("Demucs завершился с ошибкой: {}", status);
            error!("{}", error_msg);
            send_progress(&progress_sender, DemucsSeparationProgress::Error(error_msg.clone())).await;
            return Err(TtsError::AudioProcessingError(error_msg));
        }

        // Находим файл с инструментальной дорожкой
        let input_filename = input_path.as_ref().file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| TtsError::AudioProcessingError("Некорректный путь к входному файлу".to_string()))?;

        let instrumental_path = temp_dir.path()
            .join("htdemucs")
            .join(input_filename)
            .join("no_vocals.mp3");

        if !instrumental_path.exists() {
            let error_msg = "Не найден файл с инструментальной дорожкой после обработки Demucs".to_string();
            error!("{}", error_msg);
            send_progress(&progress_sender, DemucsSeparationProgress::Error(error_msg.clone())).await;
            return Err(TtsError::AudioProcessingError(error_msg));
        }

        // Отправляем статус конвертации
        send_progress(&progress_sender, DemucsSeparationProgress::Converting).await;

        // Конвертируем результат в нужный формат с помощью FFmpeg
        let output = tokio::process::Command::new("ffmpeg")
            .args(&[
                "-y",                     // Перезаписывать выходной файл
                "-i", instrumental_path.to_str().unwrap(),
                "-acodec", "pcm_s16le",   // 16-bit PCM
                "-ar", "44100",           // 44.1 кГц
                output_path.as_ref().to_str().unwrap()
            ])
            .output()
            .await
            .map_err(|e| TtsError::AudioProcessingError(format!("Ошибка запуска ffmpeg: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let error_msg = format!("Ошибка FFmpeg при конвертации результата Demucs: {}", stderr);
            error!("{}", error_msg);
            send_progress(&progress_sender, DemucsSeparationProgress::Error(error_msg.clone())).await;
            return Err(TtsError::AudioProcessingError(error_msg));
        }

        // Если указана debug_dir, сохраняем копию для отладки
        if let Some(debug_dir) = debug_dir {
            let debug_path = Path::new(debug_dir.as_ref()).join("instrumental_debug.wav");
            if let Err(e) = tokio::fs::copy(output_path.as_ref(), &debug_path).await {
                warn!("Не удалось создать отладочную копию инструментальной дорожки: {}", e);
            } else {
                info!("Создана отладочная копия инструментальной дорожки: {}", debug_path.display());
            }
        }

        info!("Вокал успешно удален с помощью Demucs: {}", output_path.as_ref().display());
        send_progress(&progress_sender, DemucsSeparationProgress::Finished).await;
        Ok(())
    }

    // Вспомогательная функция для отправки прогресса
    async fn send_progress(sender: &Option<Sender<DemucsSeparationProgress>>, progress: DemucsSeparationProgress) {
        if let Some(tx) = sender {
            let _ = tx.send(progress).await;
        }
    }

    // Функция для парсинга вывода Demucs и определения прогресса
    fn parse_demucs_progress(line: &str) -> Option<f32> {
        // Пример строки: "Processing: 45%"
        if let Some(pos) = line.find("Processing:") {
            if let Some(percent) = line[pos..].split('%').next() {
                if let Ok(value) = percent.trim_start_matches("Processing:").trim().parse::<f32>() {
                    return Some(value / 100.0);
                }
            }
        }
        None
    }

    /// Проверяет, установлен ли Demucs через pip
    pub async fn is_demucs_installed() -> bool {
        let output = tokio::process::Command::new("pip")
            .args(&["show", "demucs"])
            .output()
            .await;
            
        match output {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    }

    /// Устанавливает Demucs через pip
    pub async fn install_demucs() -> Result<()> {
        info!("Установка Demucs и зависимостей...");
        
        // Обновляем pip до последней версии
        info!("Обновление pip...");
        let output = tokio::process::Command::new("pip")
            .args(&["install", "--upgrade", "pip"])
            .output()
            .await
            .map_err(|e| TtsError::Other(anyhow::anyhow!("Ошибка обновления pip: {}", e)))?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(TtsError::Other(anyhow::anyhow!("Не удалось обновить pip: {}", error)));
        }
        
        // Устанавливаем основные зависимости
        let base_deps = [
            "numpy",
            "scipy",
            "matplotlib",
            "scikit-learn",
            "hmmlearn",
            "simplejson",
            "eyed3",
            "pydub",
            "torch",
            "torchaudio",
            "tqdm",
            "lameenc",
            "requests",
            "imbalanced-learn",
            "plotly"  // Добавляем plotly для pyAudioAnalysis
        ];
        
        for dep in base_deps.iter() {
            info!("Установка зависимости: {}", dep);
            let output = tokio::process::Command::new("pip")
                .args(&["install", "--verbose", dep])
                .output()
                .await
                .map_err(|e| TtsError::Other(anyhow::anyhow!("Ошибка установки {}: {}", dep, e)))?;
                
            if !output.status.success() {
                let error = String::from_utf8_lossy(&output.stderr);
                return Err(TtsError::Other(anyhow::anyhow!("Не удалось установить {}: {}", dep, error)));
            }
            info!("Зависимость {} успешно установлена", dep);
        }
        
        // Устанавливаем pyAudioAnalysis
        info!("Установка pyAudioAnalysis...");
        let output = tokio::process::Command::new("pip")
            .args(&[
                "install",
                "--verbose",
                "--no-cache-dir",  // Игнорируем кэш pip
                "--force-reinstall",  // Принудительная переустановка
                "pyAudioAnalysis"
            ])
            .output()
            .await
            .map_err(|e| TtsError::Other(anyhow::anyhow!("Ошибка установки pyAudioAnalysis: {}", e)))?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(TtsError::Other(anyhow::anyhow!("Не удалось установить pyAudioAnalysis: {}", error)));
        }
        info!("pyAudioAnalysis успешно установлен");
        
        // Проверяем установку pyAudioAnalysis
        info!("Проверка установки pyAudioAnalysis...");
        let output = tokio::process::Command::new("python3")
            .args(&[
                "-c",
                "from pyAudioAnalysis import ShortTermFeatures; print('pyAudioAnalysis успешно импортирован')"
            ])
            .output()
            .await
            .map_err(|e| TtsError::Other(anyhow::anyhow!("Ошибка проверки pyAudioAnalysis: {}", e)))?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(TtsError::Other(anyhow::anyhow!("Проверка pyAudioAnalysis не удалась: {}", error)));
        }
        info!("pyAudioAnalysis успешно проверен");
        
        // Устанавливаем Demucs
        info!("Установка Demucs...");
        let output = tokio::process::Command::new("pip")
            .args(&[
                "install",
                "--verbose",
                "demucs==4.0.1"
            ])
            .output()
            .await
            .map_err(|e| TtsError::Other(anyhow::anyhow!("Ошибка установки Demucs: {}", e)))?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(TtsError::Other(anyhow::anyhow!("Не удалось установить Demucs: {}", error)));
        }
        
        info!("Demucs и все зависимости успешно установлены");
        Ok(())
    }

    /// Проверяет, установлен ли Demucs, и устанавливает его при необходимости
    pub async fn ensure_demucs_installed() -> Result<()> {
        if !is_demucs_installed().await {
            info!("Demucs не установлен, начинаем установку...");
            install_demucs().await?;
        } else {
            info!("Demucs уже установлен");
        }
        Ok(())
    }
}

/// Модуль для аудио-обработки: декодирование, time-stretching, анализ громкости и кодирование.
pub mod audio {
    use super::{Result, TtsError, AudioProcessingConfig};
    use rubato::{SincFixedIn, FftFixedIn, Resampler};
    use log::{info, warn, error, debug};
    use std::path::Path;
    use tokio::sync::mpsc::Sender;
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
                "-i", path.as_ref().to_str().unwrap(),
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

    /// Применяет time-stretching к аудио для корректировки длительности.
    ///
    /// Если actual_duration > target_duration, вычисляется коэффициент ускорения:
    /// speed_factor = actual_duration / target_duration (с ограничением сверху),
    /// затем SoundTouch обрабатывает аудио чтобы итоговая длительность приблизилась к target_duration,
    /// сохраняя при этом высоту тона.
    ///
    /// Если actual_duration < target_duration, просто добавляем тишину.
    pub fn adjust_duration(
        input: &[f32],
        actual_duration: f32,
        target_duration: f32,
        available_extra_time: f32,  // Доступное дополнительное время до следующего cue
        sample_rate: u32,
        config: &AudioProcessingConfig,
    ) -> Result<(Vec<f32>, f32)> {  // Возвращаем (аудио, использованное_время)
        if input.is_empty() {
            warn!("adjust_duration получил пустой входной аудиомассив!");
            return Err(TtsError::AudioProcessingError("Попытка обработать пустое аудио".to_string()));
        }

        info!("Применение time-stretching: входное аудио {} сэмплов, actual_duration={:.3}s, target_duration={:.3}s, available_extra_time={:.3}s", 
              input.len(), actual_duration, target_duration, available_extra_time);
        
        // Проверяем, что аудио не является тишиной
        let max_amplitude = input.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        if max_amplitude < 0.001 {
            warn!("Входное аудио имеет очень низкую амплитуду: {:.6}, возможно это тишина.", max_amplitude);
            let target_samples = (target_duration * sample_rate as f32).round() as usize;
            let mut output = input.to_vec();
            if output.len() < target_samples {
                output.extend(vec![0.0; target_samples - output.len()]);
            } else {
                output.truncate(target_samples);
            }
            return Ok((output, target_duration));
        }

        // Безопасная версия - просто добавляем тишину без изменения темпа, если аудио слишком короткое
        if input.len() < 100 || actual_duration < 0.1 || target_duration < 0.1 {
            warn!("Слишком короткое аудио для time-stretching: {} сэмплов, {:.3}s -> {:.3}s, добавляем тишину", 
                  input.len(), actual_duration, target_duration);
            let target_samples = (target_duration * sample_rate as f32).round() as usize;
            let mut output = input.to_vec();
            if output.len() < target_samples {
                output.extend(vec![0.0; target_samples - output.len()]);
            } else if output.len() > target_samples {
                output.truncate(target_samples);
            }
            return Ok((output, target_duration));
        }

        // Если аудио слишком длинное, пробуем использовать дополнительное время
        if actual_duration > target_duration {
            // Вычисляем, сколько дополнительного времени мы можем использовать
            // Оставляем минимум 1 секунду до следующего cue
            let extra_time_to_use = if available_extra_time > 1.0 {
                (available_extra_time - 1.0).min(actual_duration - target_duration)
            } else {
                0.0
            };

            let extended_target = target_duration + extra_time_to_use;
            let speed_factor = actual_duration / extended_target;

            info!("Используем дополнительное время: {:.3}s, новая целевая длительность: {:.3}s, коэффициент ускорения: {:.3}", 
                  extra_time_to_use, extended_target, speed_factor);

            // Защита от слишком агрессивного ускорения
            let adjusted_speed_factor = if speed_factor > 3.0 {
                warn!("Очень высокий коэффициент ускорения ({:.2}), ограничиваем до 3.0", speed_factor);
                3.0
            } else {
                speed_factor
            };

            // Используем SoundTouch для изменения скорости с сохранением высоты тона
            match super::soundtouch::process_with_soundtouch(input, sample_rate, adjusted_speed_factor) {
                Ok(processed) => {
                    info!("Итоговое аудио после изменения скорости с сохранением тона через SoundTouch: {} сэмплов, длительность ~{:.3}s",
                          processed.len(), processed.len() as f32 / sample_rate as f32);
                    
                    // Проверим что результат не пустой
                    if processed.is_empty() {
                        warn!("SoundTouch вернул пустой результат! Используем оригинальное аудио с обрезкой.");
                        let target_samples = (extended_target * sample_rate as f32).round() as usize;
                        return Ok((input.iter().take(target_samples).cloned().collect(), extended_target));
                    }
                    
                    // Проверяем, что длительность соответствует ожидаемой
                    let result_duration = processed.len() as f32 / sample_rate as f32;
                    let target_samples = (extended_target * sample_rate as f32).round() as usize;
                    
                    if (result_duration - extended_target).abs() > 0.1 {
                        warn!("Результат обработки имеет неожиданную длительность: {:.3}s вместо {:.3}s", 
                              result_duration, extended_target);
                        
                        // Корректируем длину, если нужно
                        let mut adjusted_result = processed;
                        if adjusted_result.len() > target_samples {
                            adjusted_result.truncate(target_samples);
                        } else if adjusted_result.len() < target_samples {
                            adjusted_result.extend(vec![0.0; target_samples - adjusted_result.len()]);
                        }
                        
                        Ok((adjusted_result, extended_target))
                    } else {
                        Ok((processed, extended_target))
                    }
                },
                Err(e) => {
                    error!("Ошибка при обработке аудио через SoundTouch: {}", e);
                    
                    // Предлагаем альтернативу в случае ошибки - попробуем использовать Rubato
                    warn!("Пробуем использовать резервный метод time-stretching (Rubato FFT)");
                    
                    // Используем FftFixedIn для изменения длительности с сохранением высоты тона
                    let mut resampler = match rubato::FftFixedIn::<f32>::new(
                        sample_rate as usize,                 // Исходная частота дискретизации
                        (sample_rate as f64 / adjusted_speed_factor as f64) as usize, // Целевая частота дискретизации
                        input.len(),                          // Размер входного буфера
                        config.window_size / config.hop_size, // Количество подблоков для обработки
                        1                                     // Количество каналов (моно)
                    ) {
                        Ok(r) => r,
                        Err(e) => {
                            error!("Ошибка создания FFT-ресемплера: {}", e);
                            return Err(TtsError::TimeStretchingError(format!("Ошибка создания FFT-ресемплера: {}", e)));
                        }
                    };
                    
                    // Подготавливаем входные данные
                    let input_frames = vec![input.to_vec()];
                    
                    // Обрабатываем аудио с сохранением высоты тона
                    let output_frames = match resampler.process(&input_frames, None) {
                        Ok(p) => p,
                        Err(e) => {
                            error!("Ошибка обработки аудио через Rubato: {}", e);
                            // Безопасная альтернатива - просто обрезаем аудио
                            let target_samples = (extended_target * sample_rate as f32).round() as usize;
                            return Ok((input.iter().take(target_samples).cloned().collect(), extended_target));
                        }
                    };
                    
                    // Получаем результат (первый и единственный канал)
                    let result = output_frames[0].clone();
                    
                    info!("Итоговое аудио после изменения скорости с сохранением тона через Rubato: {} сэмплов, длительность ~{:.3}s",
                          result.len(), result.len() as f32 / sample_rate as f32);
                    
                    // Проверяем, что длительность соответствует ожидаемой
                    let result_duration = result.len() as f32 / sample_rate as f32;
                    let target_samples = (extended_target * sample_rate as f32).round() as usize;
                    
                    if (result_duration - extended_target).abs() > 0.1 {
                        // Корректируем длину, если нужно
                        let mut adjusted_result = result;
                        if adjusted_result.len() > target_samples {
                            adjusted_result.truncate(target_samples);
                        } else if adjusted_result.len() < target_samples {
                            adjusted_result.extend(vec![0.0; target_samples - adjusted_result.len()]);
                        }
                        
                        Ok((adjusted_result, extended_target))
                    } else {
                        Ok((result, extended_target))
                    }
                }
            }
        } else {
            // Если аудио короче целевого - просто добавляем тишину
            let target_samples = (target_duration * sample_rate as f32).round() as usize;
            let mut output = input.to_vec();
            if output.len() < target_samples {
                output.extend(vec![0.0; target_samples - output.len()]);
            }
            Ok((output, target_duration))
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

    /// Вычисляет RMS-уровень (корень из среднего квадрата) для набора сэмплов.
    pub fn compute_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }

    /// Удаляет голос из аудиофайла, оставляя музыку и другие звуки.
    /// По умолчанию использует Demucs для лучшего качества, с fallback на FFmpeg.
    pub async fn remove_vocals<P: AsRef<Path>>(
        input_path: P, 
        output_path: P,
        progress_sender: Option<Sender<super::demucs::DemucsSeparationProgress>>,
        debug_dir: Option<P>,
    ) -> Result<()> {
        debug!("Удаление голоса из аудио: {}", input_path.as_ref().display());
        
        // Сначала пробуем использовать Demucs
        match super::demucs::remove_vocals(&input_path, &output_path, progress_sender, debug_dir.as_ref()).await {
            Ok(_) => {
                info!("Успешно удален голос с помощью Demucs");
                return Ok(());
            },
            Err(e) => {
                warn!("Не удалось использовать Demucs ({}), переключаемся на базовый метод через FFmpeg", e);
            }
        }
        
        // Fallback на базовый метод через FFmpeg
        let output = tokio::process::Command::new("ffmpeg")
            .args(&[
                "-y",                     // Перезаписывать выходной файл
                "-i", input_path.as_ref().to_str().unwrap(),
                "-af", "pan=stereo|c0=c0-c1|c1=c1-c0,volume=2.0", // Удаление центрального канала
                "-acodec", "pcm_s16le",   // 16-bit PCM
                "-ar", "44100",           // 44.1 кГц
                output_path.as_ref().to_str().unwrap()
            ])
            .output()
            .await
            .map_err(|e| TtsError::AudioProcessingError(format!("Ошибка запуска ffmpeg: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Ошибка FFmpeg при удалении голоса: {}", stderr);
            return Err(TtsError::AudioProcessingError(format!("Ошибка FFmpeg: {}", stderr)));
        }

        info!("Голос успешно удален из аудио (базовый метод): {}", output_path.as_ref().display());
        Ok(())
    }

    /// Микширует две аудиодорожки с заданным соотношением
    pub fn mix_audio_tracks(voice: &[f32], instrumental: &[f32], voice_ratio: f32, instrumental_boost: f32) -> Vec<f32> {
        let voice_gain = voice_ratio;
        let instrumental_gain = (1.0 - voice_ratio) * instrumental_boost;
        
        let max_len = voice.len().max(instrumental.len());
        let mut mixed = Vec::with_capacity(max_len);
        
        for i in 0..max_len {
            let voice_sample = if i < voice.len() { voice[i] } else { 0.0 };
            let instrumental_sample = if i < instrumental.len() { instrumental[i] } else { 0.0 };
            
            // Микшируем с учетом усиления инструментальной дорожки
            let mixed_sample = voice_sample * voice_gain + instrumental_sample * instrumental_gain;
            
            // Предотвращаем клиппинг
            mixed.push(mixed_sample.clamp(-1.0, 1.0));
        }
        
        mixed
    }

    /// Определяет пол голоса в аудиофайле
    /// Возвращает true для мужского голоса, false для женского
    pub async fn detect_voice_gender<P: AsRef<Path>>(audio_path: P) -> Result<bool> {
        use std::process::Command;
        use std::str::from_utf8;

        info!("Определение пола голоса в аудио: {}", audio_path.as_ref().display());

        // Путь к скрипту относительно директории с исполняемым файлом
        let script_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/utils/tts/voice_gender_detector.py");

        // Проверяем существование скрипта
        if !script_path.exists() {
            error!("Скрипт определения пола голоса не найден: {}", script_path.display());
            return Err(TtsError::AudioProcessingError(
                format!("Скрипт определения пола голоса не найден: {}", script_path.display())
            ));
        }

        // Запускаем Python скрипт
        let output = Command::new("python3")
            .args(&[
                script_path.to_str().unwrap(),
                audio_path.as_ref().to_str().unwrap(),
            ])
            .output()
            .map_err(|e| TtsError::AudioProcessingError(format!("Ошибка запуска Python скрипта: {}", e)))?;

        // Всегда логируем stderr для отладки
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            debug!("Python script stderr output:\n{}", stderr);
        }

        if !output.status.success() {
            return Err(TtsError::AudioProcessingError(format!("Ошибка анализа голоса: {}", stderr)));
        }

        let result = from_utf8(&output.stdout)
            .map_err(|e| TtsError::AudioProcessingError(format!("Ошибка чтения результата: {}", e)))?
            .trim();

        match result {
            "male" => {
                info!("Определен мужской голос");
                Ok(true)
            },
            "female" => {
                info!("Определен женский голос");
                Ok(false)
            },
            _ => {
                if result.starts_with("error: ") {
                    Err(TtsError::AudioProcessingError(format!("Ошибка в Python скрипте: {}", &result[7..])))
                } else {
                    Err(TtsError::AudioProcessingError(format!("Неожиданный результат анализа: {}", result)))
                }
            }
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

    /// Структура для хранения аудиофрагмента с метаданными
    #[derive(Debug)]
    pub struct AudioFragment {
        pub samples: Vec<f32>,
        pub sample_rate: u32,
        pub text: String,
        pub start_time: f32,
        pub end_time: f32,
        pub next_cue_start: Option<f32>,  // время начала следующего cue, если есть
    }

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

        // Проверяем установку Demucs и его зависимостей (включая pyAudioAnalysis)
        info!("Проверка установки Demucs и зависимостей...");
        if let Err(e) = super::demucs::ensure_demucs_installed().await {
            error!("Не удалось установить Demucs и зависимости: {}", e);
            return Err(e);
        }
        info!("Demucs и зависимости установлены успешно");

        // Используем конфигурацию TTS как есть, без определения пола голоса
        let tts_config = config.tts_config.clone();
        info!("Используется голос {} для TTS", tts_config.voice);

        // Создаем канал для прогресса удаления вокала
        let (demucs_tx, mut demucs_rx) = tokio::sync::mpsc::channel(32);
        
        // Запускаем задачу для мониторинга прогресса Demucs
        let progress_sender = config.progress_sender.clone();
        tokio::spawn(async move {
            while let Some(progress) = demucs_rx.recv().await {
                use super::demucs::DemucsSeparationProgress::*;
                let (status, progress_value) = match progress {
                    Started => ("Удаление вокала".to_string(), 0.0),
                    LoadingModel => ("Удаление вокала".to_string(), 10.0),
                    Processing { progress } => (format!("Удаление вокала"), 10.0 + progress * 80.0),
                    Converting => ("Удаление вокала".to_string(), 90.0),
                    Finished => ("Удаление вокала завершено".to_string(), 100.0),
                    Error(ref msg) => (format!("Ошибка при удалении вокала: {}", msg), 0.0),
                };

                // For Demucs processing, we want to integrate it better with the overall TTS flow
                // We'll adjust the index/total calculation to make it fit in the 60-90% range
                // without resetting progress
                if let Some(tx) = &progress_sender {
                    // For vocal removal, we'll use special index values
                    // that won't trigger 100% progress prematurely
                    let (special_index, special_total) = match &progress {
                        super::demucs::DemucsSeparationProgress::Started => (1, 10),
                        super::demucs::DemucsSeparationProgress::LoadingModel => (2, 10),
                        super::demucs::DemucsSeparationProgress::Processing { progress: p_val } => 
                            ((2.0 + p_val * 7.0) as usize, 10),
                        super::demucs::DemucsSeparationProgress::Converting => (9, 10),
                        super::demucs::DemucsSeparationProgress::Finished => (10, 10),
                        super::demucs::DemucsSeparationProgress::Error(_) => (0, 10),
                    };
                
                    let _ = tx.send(ProgressUpdate::ProcessingFragment {
                        // Use specific index numbers that integrate with TTS flow
                        index: special_index,
                        total: special_total,
                        step: status,
                    }).await;
                }
            }
        });

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
        let tts_futures = cues.iter().enumerate().map(|(i, cue)| {
            let api_key = config.api_key;
            let text = cue.text.clone();
            let tts_config = &tts_config;
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
            
            // Обрабатываем результат генерации TTS
            let (audio_bytes, text) = tts_result.1?;
            
            // Сохраняем MP3-чанк на диск для отладки
            let sanitized_text = text.chars()
                .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' })
                .collect::<String>()
                .trim()
                .to_string();
            
            let chunk_name = format!("chunk_{:03}_{}", i, sanitized_text);
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
                    // Создаем placeholder для продолжения обработки
                    let placeholder_path = debug_dir.join(format!("{}_ERROR.txt", chunk_name));
                    let error_info = format!("Ошибка декодирования: {}\nРазмер чанка: {} байт\nТекст: {}", 
                                           e, audio_bytes.len(), text);
                    std::fs::write(placeholder_path, error_info)
                        .map_err(|e| TtsError::IoError(e))?;
                        
                    // Пропускаем этот фрагмент и продолжаем со следующим
                    continue;
                }
            };
            
            // Проверяем декодированное аудио на пустоту и уровень
            if pcm.is_empty() {
                warn!("Пустое декодированное аудио для чанка №{}. Текст: {}", i, text);
                let error_path = debug_dir.join(format!("{}_ERROR_EMPTY_PCM.txt", chunk_name));
                let error_info = format!("Пустое декодированное аудио\nРазмер MP3: {} байт\nТекст: {}", audio_bytes.len(), text);
                std::fs::write(error_path, error_info)
                    .map_err(|e| TtsError::IoError(e))?;
                continue;
            }
            
            // Проверяем уровень аудио
            let max_amplitude = pcm.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
            if max_amplitude < 0.001 {
                warn!("Очень низкий уровень аудио для чанка №{}: {:.6}. Текст: {}", i, max_amplitude, text);
                // Продолжаем обработку, но записываем предупреждение
                let warning_path = debug_dir.join(format!("{}_WARNING_LOW_LEVEL.txt", chunk_name));
                let warning_info = format!("Низкий уровень аудио: {:.6}\nРазмер MP3: {} байт\nТекст: {}", 
                                         max_amplitude, audio_bytes.len(), text);
                std::fs::write(warning_path, warning_info)
                    .map_err(|e| TtsError::IoError(e))?;
            }
            
            let actual_duration = audio::duration_in_seconds(pcm.len(), sample_rate);
            let target_duration = cue.end - cue.start;
            
            // Получаем время начала следующего cue для расчета доступного дополнительного времени
            let next_cue_start = if i < cues.len() - 1 {
                Some(cues[i + 1].start)
            } else {
                None
            };

            let available_extra_time = match next_cue_start {
                Some(next_start) => next_start - cue.end,
                None => 0.0,
            };
            
            // Сохраняем WAV после декодирования для отладки
            let wav_path = debug_dir.join(format!("{}_decoded.wav", chunk_name));
            if let Err(e) = audio::encode_wav(&pcm, sample_rate, wav_path.to_str().unwrap()) {
                warn!("Не удалось сохранить декодированный WAV для чанка №{}: {}", i, e);
            }
            
            send_progress(
                &config.progress_sender,
                ProgressUpdate::ProcessingFragment {
                    index: i + 1,
                    total: cues.len(),
                    step: format!("Длительность: target {:.3} s, actual {:.3} s", target_duration, actual_duration),
                },
            )
            .await;
            
            // Крайний случай: если целевая длительность слишком маленькая, просто добавляем короткую тишину
            let (adjusted, used_duration) = if target_duration < 0.05 {
                warn!("Очень короткая целевая длительность для чанка №{}: {:.3}s, пропускаем time-stretching", i, target_duration);
                let target_samples = (target_duration * sample_rate as f32).round() as usize;
                // Генерируем короткий сигнал с затуханием
                let mut short_signal = vec![0.0f32; target_samples];
                if !pcm.is_empty() {
                    // Копируем начало исходного аудио с затуханием
                    let copy_len = target_samples.min(pcm.len());
                    for j in 0..copy_len {
                        let fade = 1.0 - (j as f32 / copy_len as f32);
                        short_signal[j] = pcm[j] * fade;
                    }
                }
                (short_signal, target_duration)
            } else {
                // Обычная корректировка длительности
                match audio::adjust_duration(&pcm, actual_duration, target_duration, available_extra_time, sample_rate, &config.audio_config) {
                    Ok(result) => result,
                    Err(e) => {
                        error!("Ошибка при корректировке длительности чанка №{}: {}. Используем исходное аудио с добавлением тишины.", i, e);
                        // Создаем безопасную альтернативу
                        let target_samples = (target_duration * sample_rate as f32).round() as usize;
                        let mut safe_output = pcm.clone();
                        if safe_output.len() > target_samples {
                            // Если исходное аудио длиннее целевого, обрезаем
                            safe_output.truncate(target_samples);
                        } else if safe_output.len() < target_samples {
                            // Если короче, добавляем тишину
                            safe_output.extend(vec![0.0; target_samples - safe_output.len()]);
                        }
                        (safe_output, target_duration)
                    }
                }
            };
            
            // Проверяем результат корректировки длительности
            if adjusted.is_empty() {
                warn!("Пустой результат корректировки длительности для чанка №{}. Пропускаем фрагмент.", i);
                let error_path = debug_dir.join(format!("{}_ERROR_EMPTY_ADJUSTED.txt", chunk_name));
                std::fs::write(error_path, "Пустой результат корректировки длительности")
                    .map_err(|e| TtsError::IoError(e))?;
                continue;
            }
            
            // Сохраняем WAV после коррекции длительности для отладки
            let adjusted_wav_path = debug_dir.join(format!("{}_adjusted.wav", chunk_name));
            if let Err(e) = audio::encode_wav(&adjusted, sample_rate, adjusted_wav_path.to_str().unwrap()) {
                warn!("Не удалось сохранить скорректированный WAV для чанка №{}: {}", i, e);
            }
            
            // Создаем фрагмент с метаданными
            let fragment = AudioFragment {
                samples: adjusted,
                sample_rate,
                text: text.clone(),
                start_time: cue.start,
                end_time: cue.start + used_duration,
                next_cue_start,
            };
            
            // Добавляем фрагмент в итоговый набор
            info!("Успешно обработан чанк №{}: итоговая длина {} сэмплов, использованное время {:.3}s", 
                  i, fragment.samples.len(), used_duration);
            audio_fragments.push(fragment);
        }

        // 4. Склейка аудиофрагментов с учетом временных меток
        send_progress(&config.progress_sender, ProgressUpdate::MergingFragments).await;
        if audio_fragments.is_empty() {
            return Err(TtsError::AudioProcessingError("Нет аудиофрагментов для склейки".to_string()));
        }
        
        let sample_rate = audio_fragments[0].sample_rate;
        let mut final_audio = Vec::new();
        let mut current_time = 0.0;
        
        // Создаем информационный файл о каждом фрагменте
        let fragments_info_path = debug_dir.join("fragments_info.txt");
        let mut fragments_info = String::new();
        fragments_info.push_str("Информация об аудиофрагментах:\n\n");
        
        for fragment in audio_fragments.iter() {
            // Добавляем тишину, если есть пробел до начала текущего фрагмента
            if fragment.start_time > current_time {
                let silence_duration = fragment.start_time - current_time;
                let silence_samples = (silence_duration * sample_rate as f32).round() as usize;
                final_audio.extend(vec![0.0; silence_samples]);
                info!("Добавлено {:.3}s тишины перед фрагментом, начинающимся в {:.3}s", 
                      silence_duration, fragment.start_time);
            }
            
            // Добавляем сам фрагмент
            final_audio.extend_from_slice(&fragment.samples);
            current_time = fragment.end_time;
            
            // Добавляем информацию о фрагменте
            let frag_info = format!(
                "Фрагмент: start={:.3}s, end={:.3}s, duration={:.3}s, samples={}, text: {}\n",
                fragment.start_time,
                fragment.end_time,
                fragment.end_time - fragment.start_time,
                fragment.samples.len(),
                fragment.text
            );
            fragments_info.push_str(&frag_info);
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

        // Если есть оригинальное аудио, создаем инструментальную версию и микшируем
        if let Some(orig_path) = config.original_audio_path {
            info!("Создание инструментальной версии из оригинального аудио...");
            
            // Создаем временный файл для инструментальной дорожки
            let instrumental_path = debug_dir.join("instrumental.wav");
            
            // Удаляем вокал из оригинального аудио
            if let Err(e) = super::audio::remove_vocals(orig_path, &instrumental_path, Some(demucs_tx), Some(&debug_dir)).await {
                warn!("Не удалось создать инструментальную дорожку: {}. Продолжаем без нее.", e);
            } else {
                // Декодируем инструментальную дорожку
                match audio::decode_audio_file(&instrumental_path) {
                    Ok((instrumental_audio, instrumental_rate)) => {
                        if instrumental_rate != sample_rate {
                            warn!("Частота дискретизации инструментальной дорожки ({} Hz) отличается от TTS ({} Hz). Пропускаем микширование.", 
                                  instrumental_rate, sample_rate);
                        } else {
                            info!("Микширование TTS с инструментальной дорожкой...");
                            
                            // Микшируем дорожки
                            final_audio = audio::mix_audio_tracks(
                                &final_audio,
                                &instrumental_audio,
                                config.audio_config.voice_to_instrumental_ratio,
                                config.audio_config.instrumental_boost
                            );
                            
                            // Сохраняем микшированную версию для отладки
                            let mixed_debug_path = debug_dir.join("final_mixed.wav");
                            if let Err(e) = audio::encode_wav(&final_audio, sample_rate, mixed_debug_path.to_str().unwrap()) {
                                warn!("Не удалось сохранить микшированный WAV для отладки: {}", e);
                            }

                            // Сохраняем финальный микшированный результат
                            info!("Сохранение финального микшированного аудио...");
                            if let Err(e) = audio::encode_wav(&final_audio, sample_rate, config.output_wav.to_str().unwrap()) {
                                error!("Ошибка при сохранении финального микшированного WAV: {}", e);
                                return Err(e.into());
                            }
                            info!("Финальное микшированное аудио успешно сохранено: {}", config.output_wav.display());
                        }
                    },
                    Err(e) => warn!("Не удалось декодировать инструментальную дорожку: {}. Продолжаем без микширования.", e),
                }
            }
        }

        Ok(())
    }
}
