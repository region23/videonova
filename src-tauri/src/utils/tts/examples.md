# Примеры использования TTS системы VideoNova

В этом документе приведены примеры использования модульной TTS системы для различных сценариев.

## 1. Базовая генерация речи из текста

Самый простой пример - генерация TTS аудио из VTT-файла:

```rust
use crate::utils::tts::{
    SyncConfig, TtsVoiceConfig, AudioProcessingConfig, synchronize_tts
};
use std::path::PathBuf;

async fn generate_speech(
    vtt_path: &str,
    output_path: &str,
    api_key: &str
) -> Result<PathBuf, String> {
    // 1. Создаем конфигурацию голоса с параметрами по умолчанию
    let tts_config = TtsVoiceConfig::default();

    // 2. Создаем конфигурацию обработки аудио
    let audio_config = AudioProcessingConfig::default();
    
    // 3. Создаем объект конфигурации синхронизации
    let config = SyncConfig {
        vtt_path,
        output_wav: PathBuf::from(output_path),
        api_key,
        tts_config,
        audio_config,
        original_audio_path: None,
        progress_sender: None,
        debug_dir: None,
    };
    
    // 4. Вызываем функцию генерации TTS
    synchronize_tts(config)
        .await
        .map_err(|e| format!("Ошибка генерации TTS: {:?}", e))
}
```

## 2. Настройка параметров голоса и обработки аудио

Пример с настройкой различных параметров:

```rust
use crate::utils::tts::{
    SyncConfig, TtsVoiceConfig, AudioProcessingConfig, synchronize_tts
};
use std::path::PathBuf;

async fn generate_custom_speech(
    vtt_path: &str,
    output_path: &str,
    api_key: &str
) -> Result<PathBuf, String> {
    // 1. Настраиваем голос
    let tts_config = TtsVoiceConfig {
        model: "tts-1-hd".to_string(),   // Высококачественная модель
        voice: "alloy".to_string(),      // Голос alloy
        speed: 1.2,                       // Скорость немного выше обычной
    };

    // 2. Настраиваем обработку аудио
    let audio_config = AudioProcessingConfig {
        target_peak_level: 0.9,          // Целевой уровень громкости
        voice_to_instrumental_ratio: 1.8, // Голос будет заметно громче фоновой музыки
        instrumental_boost: 1.2,         // Небольшое усиление фоновой музыки
        max_rubato_speed: 2.0,           // Максимальное ускорение для Rubato
        min_rubato_speed: 0.6,           // Минимальное замедление для Rubato 
        max_soundtouch_speed: 2.5,       // Максимальное ускорение для SoundTouch
        min_soundtouch_speed: 0.4,       // Минимальное замедление для SoundTouch
        extra_time_usage_factor: 0.5,    // Использование дополнительного времени
    };
    
    // 3. Создаем конфигурацию синхронизации
    let config = SyncConfig {
        vtt_path,
        output_wav: PathBuf::from(output_path),
        api_key,
        tts_config,
        audio_config,
        original_audio_path: None,
        progress_sender: None,
        debug_dir: None,
    };
    
    // 4. Вызываем функцию генерации TTS
    synchronize_tts(config)
        .await
        .map_err(|e| format!("Ошибка генерации TTS: {:?}", e))
}
```

## 3. Синхронизация с оригинальным аудио

Пример с указанием оригинального аудио для нормализации:

```rust
use crate::utils::tts::{
    SyncConfig, TtsVoiceConfig, AudioProcessingConfig, synchronize_tts
};
use std::path::PathBuf;

async fn generate_synced_speech(
    vtt_path: &str,
    output_path: &str,
    api_key: &str,
    original_audio_path: &str,
) -> Result<PathBuf, String> {
    // 1. Создаем конфигурацию 
    let config = SyncConfig {
        vtt_path,
        output_wav: PathBuf::from(output_path),
        api_key,
        tts_config: TtsVoiceConfig::default(),
        audio_config: AudioProcessingConfig::default(),
        original_audio_path: Some(original_audio_path),
        progress_sender: None,
        debug_dir: None,
    };
    
    // 2. Вызываем функцию генерации TTS
    synchronize_tts(config)
        .await
        .map_err(|e| format!("Ошибка генерации TTS: {:?}", e))
}
```

## 4. Мониторинг прогресса генерации

Пример мониторинга прогресса выполнения:

```rust
use crate::utils::tts::{
    SyncConfig, TtsVoiceConfig, AudioProcessingConfig, synchronize_tts,
    ProgressUpdate
};
use std::path::PathBuf;
use tokio::sync::mpsc;

async fn generate_speech_with_progress(
    vtt_path: &str,
    output_path: &str,
    api_key: &str
) -> Result<PathBuf, String> {
    // 1. Создаем канал для получения обновлений прогресса
    let (tx, mut rx) = mpsc::channel(32);
    
    // 2. Запускаем задачу для обработки обновлений прогресса
    let progress_task = tokio::spawn(async move {
        while let Some(update) = rx.recv().await {
            match update {
                ProgressUpdate::Started => {
                    println!("Начало генерации TTS");
                },
                ProgressUpdate::ParsingVTT => {
                    println!("Парсинг VTT-файла");
                },
                ProgressUpdate::TTSGeneration { current, total } => {
                    println!("Генерация TTS: {}/{}", current, total);
                },
                ProgressUpdate::ProcessingFragment { index, total, step } => {
                    println!("Обработка фрагмента {}/{}: {}", index, total, step);
                },
                ProgressUpdate::MergingFragments => {
                    println!("Объединение фрагментов");
                },
                ProgressUpdate::Normalizing { using_original } => {
                    if using_original {
                        println!("Нормализация относительно оригинального аудио");
                    } else {
                        println!("Нормализация аудио");
                    }
                },
                ProgressUpdate::Encoding => {
                    println!("Кодирование аудио");
                },
                ProgressUpdate::Finished => {
                    println!("Генерация TTS завершена");
                },
                ProgressUpdate::Error(err) => {
                    println!("Ошибка: {}", err);
                },
            }
        }
    });
    
    // 3. Создаем конфигурацию с отправителем прогресса
    let config = SyncConfig {
        vtt_path,
        output_wav: PathBuf::from(output_path),
        api_key,
        tts_config: TtsVoiceConfig::default(),
        audio_config: AudioProcessingConfig::default(),
        original_audio_path: None,
        progress_sender: Some(tx),
        debug_dir: None,
    };
    
    // 4. Вызываем функцию генерации TTS
    let result = synchronize_tts(config)
        .await
        .map_err(|e| format!("Ошибка генерации TTS: {:?}", e));
    
    // 5. Отменяем задачу мониторинга прогресса
    progress_task.abort();
    
    result
}
```

## 5. Обработка ошибок

Пример с подробной обработкой ошибок:

```rust
use crate::utils::tts::{
    SyncConfig, TtsVoiceConfig, AudioProcessingConfig, synchronize_tts,
    TtsError
};
use std::path::PathBuf;

async fn generate_speech_with_error_handling(
    vtt_path: &str,
    output_path: &str,
    api_key: &str
) -> Result<PathBuf, String> {
    // 1. Создаем конфигурацию
    let config = SyncConfig {
        vtt_path,
        output_wav: PathBuf::from(output_path),
        api_key,
        tts_config: TtsVoiceConfig::default(),
        audio_config: AudioProcessingConfig::default(),
        original_audio_path: None,
        progress_sender: None,
        debug_dir: None,
    };
    
    // 2. Вызываем функцию с обработкой различных типов ошибок
    match synchronize_tts(config).await {
        Ok(path) => Ok(path),
        
        Err(TtsError::VttParsingError(msg)) => {
            Err(format!("Ошибка при парсинге VTT-файла: {}", msg))
        },
        
        Err(TtsError::OpenAIApiError(msg)) => {
            Err(format!("Ошибка OpenAI API: {}", msg))
        },
        
        Err(TtsError::HttpError(e)) => {
            Err(format!("Ошибка HTTP-запроса: {}", e))
        },
        
        Err(TtsError::AudioProcessingError(msg)) => {
            Err(format!("Ошибка обработки аудио: {}", msg))
        },
        
        Err(TtsError::TimeStretchingError(msg)) => {
            Err(format!("Ошибка изменения темпа: {}", msg))
        },
        
        Err(TtsError::IoError(e)) => {
            Err(format!("Ошибка ввода/вывода: {}", e))
        },
        
        Err(TtsError::WavEncodingError(e)) | Err(TtsError::WavDecodingError(e)) => {
            Err(format!("Ошибка кодирования/декодирования WAV: {}", e))
        },
        
        Err(TtsError::ConfigError(msg)) => {
            Err(format!("Ошибка конфигурации: {}", msg))
        },
        
        Err(e) => {
            Err(format!("Неизвестная ошибка: {:?}", e))
        }
    }
}
```

## 6. Удаление вокала из аудио

Пример использования функции разделения аудио:

```rust
use crate::utils::tts::separate_audio;
use std::path::PathBuf;

async fn remove_vocals_from_audio(
    input_path: &str,
    output_dir: &str
) -> Result<(PathBuf, PathBuf), String> {
    // Запускаем процесс разделения аудио на вокал и инструментал
    separate_audio(
        PathBuf::from(input_path),
        PathBuf::from(output_dir),
        None // Без отправки прогресса
    )
    .await
    .map_err(|e| format!("Ошибка разделения аудио: {:?}", e))
}
```

## 7. Анализ субтитров

Пример анализа субтитров для выявления проблемных мест:

```rust
use crate::utils::tts::{
    vtt, analyze_segments, SegmentAnalysisConfig
};

async fn analyze_subtitle_segments(vtt_path: &str) -> Result<(), String> {
    // 1. Парсим VTT-файл
    let cues = vtt::parse_vtt(vtt_path)
        .map_err(|e| format!("Ошибка парсинга VTT: {:?}", e))?;
    
    // 2. Настраиваем параметры анализа
    let config = SegmentAnalysisConfig {
        target_words_per_second: 2.5, // Целевая скорость речи
        max_words_per_second: 3.5,    // Максимальная допустимая скорость
    };
    
    // 3. Анализируем сегменты
    let results = analyze_segments(&cues, &config);
    
    // 4. Выводим результаты анализа
    for result in results {
        if result.severity > 0 {
            println!("Проблемный сегмент #{}: {} слов за {:.1} сек", 
                result.index + 1, 
                result.word_count, 
                result.duration);
            println!("  Скорость: {:.1} слов/сек (требуется ускорение: {:.2}x)",
                result.words_per_second,
                result.required_speed_factor);
            println!("  Критичность: {}/10", result.severity);
            
            // Выводим текст субтитра, если он проблемный
            if result.index < cues.len() {
                println!("  Текст: {}", cues[result.index].text);
            }
        }
    }
    
    Ok(())
}
```

## Заключение

При использовании модульного API TTS системы рекомендуется:

1. Работать напрямую с конкретными модулями для специфической функциональности
2. Использовать `synchronize_tts` для полного процесса генерации TTS
3. Конфигурировать процесс через структуры `TtsVoiceConfig` и `AudioProcessingConfig`
4. Отслеживать прогресс через систему обновлений `ProgressUpdate`
5. Обрабатывать ошибки с использованием типа `TtsError`

Дополнительную информацию можно найти в документации к соответствующим модулям. 