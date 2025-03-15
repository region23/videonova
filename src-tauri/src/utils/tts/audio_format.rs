//! # Audio Format Handling
//! 
//! Модуль для кодирования и декодирования различных аудио форматов.
//! Предоставляет функционал для работы с WAV, MP3 и другими форматами.
//!
//! ## Основные возможности
//! 
//! - Декодирование аудиофайлов популярных форматов (WAV, MP3, AAC, FLAC, OGG)
//! - Кодирование PCM данных в WAV формат
//! - Обработка многоканального аудио с конвертацией в моно
//! - Вычисление аудио-метрик, таких как RMS (среднеквадратичное значение)
//!
//! ## Примеры использования
//!
//! ```rust
//! // Декодирование MP3 в PCM семплы
//! let (samples, sample_rate) = decode_audio_file("input.mp3")?;
//! 
//! // Обработка аудио...
//! 
//! // Кодирование обратно в WAV
//! encode_wav(&samples, sample_rate, "output.wav")?;
//! ```

use std::path::Path;
use std::fs::File;
use std::io::Read;
use log::{info, warn, error};
use hound::{WavReader, WavWriter, WavSpec, SampleFormat};
use symphonia::core::audio::{SampleBuffer, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::utils::tts::types::{TtsError, Result};

/// Вычисляет длительность аудио в секундах на основе количества семплов и частоты дискретизации.
///
/// # Аргументы
///
/// * `sample_count` - Количество семплов в аудио
/// * `sample_rate` - Частота дискретизации аудио в Гц
///
/// # Возвращает
///
/// Длительность в секундах
///
/// # Примеры
///
/// ```rust
/// let duration = duration_in_seconds(44100, 44100); // 1.0 секунда
/// let duration = duration_in_seconds(88200, 44100); // 2.0 секунды
/// ```
pub fn duration_in_seconds(sample_count: usize, sample_rate: u32) -> f32 {
    sample_count as f32 / sample_rate as f32
}

/// Декодирует MP3 данные в PCM семплы.
/// 
/// Преобразует бинарные MP3 данные в несжатые PCM семплы с плавающей точкой.
/// Если аудио многоканальное, выполняется микширование каналов в моно.
/// 
/// # Аргументы
/// 
/// * `mp3_data` - Бинарные данные MP3
/// 
/// # Возвращает
/// 
/// Кортеж из вектора PCM семплов (f32) и частоты дискретизации (u32)
/// 
/// # Ошибки
/// 
/// Возвращает ошибку TtsError::AudioProcessingError если:
/// * Не удалось проверить формат данных
/// * Не найден аудио-трек в MP3
/// * Не удалось создать декодер
/// 
/// # Примеры
/// 
/// ```rust
/// let mp3_data = std::fs::read("audio.mp3")?;
/// let (pcm_samples, sample_rate) = decode_mp3(&mp3_data)?;
/// ```
pub fn decode_mp3(mp3_data: &[u8]) -> Result<(Vec<f32>, u32)> {
    // Создаем клон данных для владения
    let mp3_data_owned = mp3_data.to_vec();
    
    // Создаем источник данных
    let cursor = std::io::Cursor::new(mp3_data_owned);
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
    
    // Создаем опции проб
    let format_opts = FormatOptions {
        enable_gapless: false,
        ..Default::default()
    };
    
    // Пробуем формат
    let probed = symphonia::default::get_probe()
        .format(&Hint::new(), mss, &format_opts, &Default::default())
        .map_err(|e| TtsError::AudioProcessingError(format!("Ошибка проверки формата: {}", e)))?;
    
    // Получаем формат и первый аудио-трек
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| TtsError::AudioProcessingError("Не найден аудио-трек в MP3".to_string()))?;
    
    // Создаем декодер для трека
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions { verify: true })
        .map_err(|e| TtsError::AudioProcessingError(format!("Не удалось создать декодер MP3: {}", e)))?;
    
    // Получаем параметры аудио
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let channels = track.codec_params.channels.unwrap_or_default().count();
    
    // Готовим буфер для семплов
    let mut pcm_data = Vec::new();
    
    // Декодируем пакеты
    while let Ok(packet) = format.next_packet() {
        // Пропускаем пакеты, не относящиеся к нашему треку
        if packet.track_id() != track_id {
            continue;
        }
        
        // Декодируем пакет
        match decoder.decode(&packet) {
            Ok(decoded) => {
                // Создаем буфер для семплов
                let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                
                // Наполняем буфер семплами
                sample_buf.copy_planar_ref(decoded);
                
                // Получаем все семплы как срез
                let samples = sample_buf.samples();
                
                // Объединяем каналы в моно, если их больше одного
                if channels > 1 {
                    // Количество семплов на канал
                    let frames_per_channel = samples.len() / channels as usize;
                    
                    for frame in 0..frames_per_channel {
                        let mut sum = 0.0;
                        for ch in 0..channels as usize {
                            // Индекс семпла для текущего канала и фрейма
                            let sample_index = ch * frames_per_channel + frame;
                            sum += samples[sample_index];
                        }
                        pcm_data.push(sum / channels as f32);
                    }
                } else {
                    // Просто копируем семплы, если аудио моно
                    pcm_data.extend_from_slice(samples);
                }
            },
            Err(e) => {
                warn!("Ошибка декодирования пакета MP3: {}", e);
                // Пропускаем проблемный пакет и продолжаем
                continue;
            }
        }
    }
    
    info!("Декодировано {} семплов MP3 с частотой {}", pcm_data.len(), sample_rate);
    Ok((pcm_data, sample_rate))
}

/// Декодирует аудиофайл разных форматов в PCM семплы.
/// 
/// Поддерживает форматы:
/// * WAV - через специализированный декодер
/// * MP3, M4A, AAC, FLAC, OGG - через универсальный декодер Symphonia
/// 
/// Автоматически определяет формат по расширению файла и применяет соответствующий декодер.
/// Для многоканального аудио выполняется микширование в моно.
/// 
/// # Аргументы
/// 
/// * `file_path` - Путь к аудиофайлу
/// 
/// # Возвращает
/// 
/// Кортеж из вектора с PCM-семплами (f32) и частоты дискретизации (u32)
/// 
/// # Ошибки
/// 
/// Возвращает ошибку в случаях:
/// * Файл не существует или недоступен (TtsError::IoError)
/// * Неподдерживаемый формат (TtsError::AudioProcessingError)
/// * Ошибки декодирования (TtsError::AudioProcessingError)
/// 
/// # Примеры
/// 
/// ```rust
/// // Декодирование WAV файла
/// let (samples, sample_rate) = decode_audio_file("input.wav")?;
/// 
/// // Декодирование MP3 файла
/// let (samples, sample_rate) = decode_audio_file("music.mp3")?;
/// ```
pub fn decode_audio_file<P: AsRef<Path>>(file_path: P) -> Result<(Vec<f32>, u32)> {
    let file_path = file_path.as_ref();
    let extension = file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    // Открываем файл
    let mut file = File::open(file_path)
        .map_err(|e| TtsError::IoError(e))?;
    
    match extension.as_str() {
        "wav" => decode_wav_file(file_path),
        
        "mp3" | "m4a" | "aac" | "flac" | "ogg" => {
            // Читаем весь файл в память
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)
                .map_err(|e| TtsError::IoError(e))?;
            
            // Используем symphonia для декодирования
            let cursor = std::io::Cursor::new(buffer);
            
            // Создаем источник данных
            let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
            
            // Создаем подсказку для формата
            let mut hint = Hint::new();
            hint.with_extension(&extension);
            
            // Пробуем распознать формат
            let probed = symphonia::default::get_probe()
                .format(&hint, mss, &Default::default(), &Default::default())
                .map_err(|e| TtsError::AudioProcessingError(format!("Не удалось определить формат аудио: {}", e)))?;
            
            // Получаем формат и первый аудио-трек
            let mut format = probed.format;
            let track = format
                .tracks()
                .iter()
                .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
                .ok_or_else(|| TtsError::AudioProcessingError("Не найден аудио-трек".to_string()))?;
            
            // Создаем декодер для трека
            let mut decoder = symphonia::default::get_codecs()
                .make(&track.codec_params, &Default::default())
                .map_err(|e| TtsError::AudioProcessingError(format!("Не удалось создать декодер: {}", e)))?;
            
            // Получаем параметры аудио
            let track_id = track.id;
            let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
            let channels = track.codec_params.channels.unwrap_or_default().count();
            
            // Готовим буфер для семплов
            let mut pcm_data = Vec::new();
            
            // Декодируем пакеты
            while let Ok(packet) = format.next_packet() {
                // Пропускаем пакеты, не относящиеся к нашему треку
                if packet.track_id() != track_id {
                    continue;
                }
                
                // Декодируем пакет
                if let Ok(decoded) = decoder.decode(&packet) {
                    // Создаем буфер для семплов
                    let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                    
                    // Наполняем буфер семплами
                    sample_buf.copy_planar_ref(decoded);
                    
                    // Получаем все семплы как срез
                    let samples = sample_buf.samples();
                    
                    // Объединяем каналы в моно, если их больше одного
                    if channels > 1 {
                        // Количество семплов на канал
                        let frames_per_channel = samples.len() / channels as usize;
                        
                        for frame in 0..frames_per_channel {
                            let mut sum = 0.0;
                            for ch in 0..channels as usize {
                                // Индекс семпла для текущего канала и фрейма
                                let sample_index = ch * frames_per_channel + frame;
                                sum += samples[sample_index];
                            }
                            pcm_data.push(sum / channels as f32);
                        }
                    } else {
                        // Просто копируем семплы, если аудио моно
                        pcm_data.extend_from_slice(samples);
                    }
                }
            }
            
            info!("Декодировано {} семплов из файла {} с частотой {}", pcm_data.len(), file_path.display(), sample_rate);
            Ok((pcm_data, sample_rate))
        },
        
        _ => Err(TtsError::AudioProcessingError(format!("Неподдерживаемый формат аудио: {}", extension)))
    }
}

/// Декодирует WAV-файл в PCM семплы.
/// 
/// Специализированная функция для декодирования WAV файлов с использованием
/// библиотеки hound. Поддерживает различные форматы WAV (8/16/24/32 бит, 
/// целочисленные и с плавающей точкой).
/// 
/// # Аргументы
/// 
/// * `file_path` - Путь к WAV-файлу
/// 
/// # Возвращает
/// 
/// Кортеж из вектора с PCM-семплами (f32) и частоты дискретизации (u32)
/// 
/// # Ошибки
/// 
/// Возвращает ошибку TtsError::WavDecodingError если:
/// * Файл не открывается
/// * Формат файла некорректен
/// * Произошла ошибка при чтении семплов
/// 
/// # Примеры
/// 
/// ```rust
/// let (samples, sample_rate) = decode_wav_file("recording.wav")?;
/// println!("Длительность: {}с", duration_in_seconds(samples.len(), sample_rate));
/// ```
pub fn decode_wav_file<P: AsRef<Path>>(file_path: P) -> Result<(Vec<f32>, u32)> {
    // Открываем WAV-файл
    let mut reader = WavReader::open(file_path.as_ref())
        .map_err(|e| TtsError::WavDecodingError(e))?;
    
    let spec = reader.spec();
    let sample_rate = spec.sample_rate;
    
    // Читаем данные в зависимости от формата
    let pcm_data: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (SampleFormat::Int, 16) => {
            reader.samples::<i16>()
                .map(|s| s.map_err(|e| TtsError::WavDecodingError(e)))
                .collect::<Result<Vec<i16>>>()?
                .into_iter()
                .map(|s| s as f32 / 32768.0)
                .collect()
        },
        (SampleFormat::Int, 24) => {
            reader.samples::<i32>()
                .map(|s| s.map_err(|e| TtsError::WavDecodingError(e)))
                .collect::<Result<Vec<i32>>>()?
                .into_iter()
                .map(|s| s as f32 / 8388608.0)
                .collect()
        },
        (SampleFormat::Int, 32) => {
            reader.samples::<i32>()
                .map(|s| s.map_err(|e| TtsError::WavDecodingError(e)))
                .collect::<Result<Vec<i32>>>()?
                .into_iter()
                .map(|s| s as f32 / 2147483648.0)
                .collect()
        },
        (SampleFormat::Float, 32) => {
            reader.samples::<f32>()
                .map(|s| s.map_err(|e| TtsError::WavDecodingError(e)))
                .collect::<Result<Vec<f32>>>()?
        },
        _ => {
            return Err(TtsError::AudioProcessingError(
                format!(
                    "Неподдерживаемый формат WAV: {:?}, {} бит", 
                    spec.sample_format, 
                    spec.bits_per_sample
                )
            ));
        }
    };
    
    // Если больше одного канала, сводим к моно
    let channels = spec.channels as usize;
    if channels > 1 {
        let mut mono_data = Vec::with_capacity(pcm_data.len() / channels);
        for chunk in pcm_data.chunks(channels) {
            let sample = chunk.iter().sum::<f32>() / channels as f32;
            mono_data.push(sample);
        }
        Ok((mono_data, sample_rate))
    } else {
        Ok((pcm_data, sample_rate))
    }
}

/// Кодирует PCM семплы в WAV-файл.
/// 
/// Записывает несжатые аудио данные в WAV-файл с заданной частотой дискретизации.
/// Использует формат 32-бит с плавающей точкой для максимального качества.
/// 
/// # Аргументы
/// 
/// * `pcm_data` - Вектор PCM семплов в формате f32 (диапазон [-1.0, 1.0])
/// * `sample_rate` - Частота дискретизации в Гц
/// * `output_path` - Путь для сохранения WAV-файла
/// 
/// # Возвращает
/// 
/// Ok(()) при успешном сохранении, иначе ошибку TtsError
/// 
/// # Ошибки
/// 
/// Возвращает ошибку TtsError::WavEncodingError если:
/// * Не удалось создать выходной файл
/// * Произошла ошибка при записи WAV-заголовка
/// * Произошла ошибка при записи семплов
/// 
/// # Примеры
/// 
/// ```rust
/// // Создание синусоидального сигнала 440Гц
/// let sample_rate = 44100;
/// let duration = 2.0; // 2 секунды
/// let mut samples = Vec::with_capacity((sample_rate as f32 * duration) as usize);
/// 
/// for i in 0..(sample_rate as f32 * duration) as usize {
///     let t = i as f32 / sample_rate as f32;
///     samples.push((t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5);
/// }
/// 
/// // Сохранение в WAV
/// encode_wav(&samples, sample_rate, "sine_440hz.wav")?;
/// ```
pub fn encode_wav(pcm_data: &[f32], sample_rate: u32, output_path: &str) -> Result<()> {
    // Создаем спецификацию WAV-файла
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    
    // Создаем писателя WAV-файла
    let mut writer = WavWriter::create(output_path, spec)
        .map_err(|e| TtsError::WavEncodingError(e))?;
    
    // Записываем данные
    for &sample in pcm_data {
        writer.write_sample(sample)
            .map_err(|e| TtsError::WavEncodingError(e))?;
    }
    
    // Закрываем писателя
    writer.finalize()
        .map_err(|e| TtsError::WavEncodingError(e))?;
    
    info!("Сохранен WAV-файл: {} ({} семплов, {} Гц)", output_path, pcm_data.len(), sample_rate);
    Ok(())
}

/// Вычисляет среднеквадратичное значение (RMS) для массива семплов.
/// 
/// RMS является мерой средней амплитуды сигнала и часто используется
/// для измерения громкости аудио.
/// 
/// # Аргументы
/// 
/// * `samples` - Срез PCM семплов в формате f32
/// 
/// # Возвращает
/// 
/// Значение RMS (больше значение = громче звук)
/// 
/// # Примеры
/// 
/// ```rust
/// let samples = vec![0.5, -0.5, 0.5, -0.5];
/// let rms = compute_rms(&samples); // Примерно 0.5
/// 
/// // Использование для нормализации
/// let target_rms = 0.3;
/// let current_rms = compute_rms(&samples);
/// let gain = target_rms / current_rms;
/// 
/// // Применение коэффициента усиления ко всем семплам
/// let normalized_samples: Vec<f32> = samples.iter().map(|s| s * gain).collect();
/// ```
pub fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    
    let sum_squares: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_squares / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_duration_calculation() {
        assert_eq!(duration_in_seconds(44100, 44100), 1.0);
        assert_eq!(duration_in_seconds(22050, 44100), 0.5);
        assert_eq!(duration_in_seconds(0, 44100), 0.0);
    }
    
    #[test]
    fn test_compute_rms() {
        let samples = vec![0.0, 0.5, -0.5, 1.0, -1.0];
        // RMS = sqrt((0² + 0.5² + (-0.5)² + 1² + (-1)²) / 5) = sqrt(2.5 / 5) = sqrt(0.5) ≈ 0.7071
        assert!((compute_rms(&samples) - 0.7071).abs() < 0.0001);
        
        // Проверка пустого вектора
        assert_eq!(compute_rms(&[]), 0.0);
    }
    
    #[test]
    fn test_wav_encode_decode() {
        // Создаем временную директорию
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.wav");
        
        // Генерируем тестовые данные (синусоида 440 Гц)
        let sample_rate = 44100;
        let duration = 0.1; // 100 ms
        let num_samples = (sample_rate as f32 * duration) as usize;
        let mut samples = Vec::with_capacity(num_samples);
        
        for i in 0..num_samples {
            let time = i as f32 / sample_rate as f32;
            let sample = (time * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
            samples.push(sample);
        }
        
        // Кодируем в WAV
        let path_str = file_path.to_str().unwrap();
        encode_wav(&samples, sample_rate, path_str).unwrap();
        
        // Декодируем обратно
        let (decoded, decoded_rate) = decode_wav_file(&file_path).unwrap();
        
        // Проверяем результаты
        assert_eq!(decoded_rate, sample_rate);
        assert_eq!(decoded.len(), samples.len());
        
        // Сравниваем семплы с некоторой погрешностью
        for (a, b) in samples.iter().zip(decoded.iter()) {
            assert!((a - b).abs() < 0.0001);
        }
    }
} 