# TTS (Text-to-Speech) System

## Обзор

TTS система предназначена для генерации речи из текстовых субтитров с возможностью синхронизации с оригинальным аудио. Система использует OpenAI TTS API для генерации высококачественного голоса и включает инструменты для анализа, обработки и синхронизации аудио.

## Архитектура

Система имеет модульную архитектуру, что обеспечивает легкость тестирования, расширения и поддержки кода. Каждый модуль выполняет определенную функцию и может использоваться независимо.

### Основные модули

![Архитектура TTS системы](https://excalidraw.com/excalidraw.png)

#### 1. `synchronizer.rs`

Центральный модуль, координирующий работу всей системы. Объединяет функциональность других модулей для обеспечения полного цикла от парсинга субтитров до генерации выходного аудио.

**Основные функции:**
- Координация процесса синхронизации
- Управление генерацией фрагментов
- Объединение аудиофрагментов с учетом таймингов

#### 2. `vtt.rs`

Модуль для работы с VTT (WebVTT) субтитрами.

**Основные функции:**
- Парсинг VTT файлов
- Анализ субтитров на проблемы с таймингом
- Оптимизация распределения времени между субтитрами

#### 3. `openai_tts.rs`

Модуль для взаимодействия с OpenAI TTS API.

**Основные функции:**
- Отправка запросов к TTS API
- Обработка ответов и ошибок
- Управление голосами и моделями

#### 4. `audio_processing.rs`

Модуль для обработки аудиоданных.

**Основные функции:**
- Изменение длительности аудио (time-stretching)
- Нормализация громкости
- Применение fade in/out
- Микширование аудиодорожек

#### 5. `audio_format.rs`

Модуль для работы с различными аудиоформатами.

**Основные функции:**
- Декодирование WAV, MP3 и других форматов
- Кодирование в WAV
- Расчет аудио-метрик (RMS, длительность)

#### 6. `soundtouch.rs`

Модуль-обертка для библиотеки SoundTouch, используемой для изменения темпа аудио.

**Основные функции:**
- Изменение скорости аудио без изменения высоты тона
- Обработка коротких аудиофрагментов

#### 7. `demucs.rs`

Модуль для разделения аудио на голос и инструментал.

**Основные функции:**
- Отделение вокала от музыки
- Сохранение разделенных дорожек

#### 8. `types.rs`

Модуль с определениями общих типов данных и структур.

**Основные типы:**
- `TtsVoiceConfig` - конфигурация голоса
- `AudioProcessingConfig` - настройки обработки аудио
- `ProgressUpdate` - структуры для отслеживания прогресса
- `TtsError` - типы ошибок

## Использование системы

### Базовый пример

```rust
use crate::utils::tts::synchronizer::TtsSynchronizer;
use crate::utils::tts::types::{SyncConfig, TtsVoiceConfig, AudioProcessingConfig};
use std::path::PathBuf;

async fn generate_tts() -> Result<()> {
    // Создаем конфигурацию голоса
    let voice_config = TtsVoiceConfig {
        model: "tts-1".to_string(),
        voice: "alloy".to_string(),
        speed: 1.0,
    };
    
    // Создаем конфигурацию обработки аудио
    let audio_config = AudioProcessingConfig::default();
    
    // Создаем конфигурацию синхронизации
    let config = SyncConfig {
        vtt_path: "subtitles.vtt",
        output_wav: PathBuf::from("output.wav"),
        original_audio_path: Some("original.mp3"),
        api_key: "your-openai-api-key",
        tts_config: voice_config,
        audio_config,
        progress_sender: None,
        debug_dir: None,
    };
    
    // Создаем синхронизатор
    let mut synchronizer = TtsSynchronizer::new(config);
    
    // Запускаем процесс синхронизации
    let output_path = synchronizer.synchronize().await?;
    
    println!("TTS успешно сгенерирован: {:?}", output_path);
    Ok(())
}
```

### Отслеживание прогресса

```rust
use crate::utils::tts::types::ProgressUpdate;
use tokio::sync::mpsc;

async fn generate_tts_with_progress() -> Result<()> {
    // Создаем канал для отслеживания прогресса
    let (tx, mut rx) = mpsc::channel(10);
    
    // Создаем конфигурацию с отправителем прогресса
    let config = SyncConfig {
        // ... другие поля ...
        progress_sender: Some(tx),
    };
    
    // Запускаем синхронизацию в отдельной задаче
    let handle = tokio::spawn(async move {
        let mut synchronizer = TtsSynchronizer::new(config);
        synchronizer.synchronize().await
    });
    
    // Отслеживаем прогресс
    while let Some(update) = rx.recv().await {
        match update {
            ProgressUpdate::Started => println!("Процесс начат"),
            ProgressUpdate::ParsingVTT => println!("Парсинг VTT"),
            ProgressUpdate::TTSGeneration { current, total } => 
                println!("Генерация TTS: {}/{}", current, total),
            ProgressUpdate::MergingFragments => println!("Объединение фрагментов"),
            ProgressUpdate::ProcessingFragment { index, total, step } => 
                println!("Обработка фрагмента {}/{}: {}", index, total, step),
            ProgressUpdate::Encoding => println!("Кодирование WAV"),
            ProgressUpdate::Finished => println!("Процесс завершен"),
            ProgressUpdate::Error(err) => println!("Ошибка: {}", err),
        }
    }
    
    // Получаем результат
    let result = handle.await??;
    println!("TTS успешно сгенерирован: {:?}", result);
    Ok(())
}
```

## Работа с отдельными модулями

### Парсинг VTT-файлов

```rust
use crate::utils::tts::vtt;
use crate::utils::tts::types::SegmentAnalysisConfig;

// Парсинг VTT
let cues = vtt::parse_vtt("subtitles.vtt")?;

// Анализ субтитров
let config = SegmentAnalysisConfig::default();
let analysis = vtt::analyze_segments(&cues, &config);

// Оптимизация распределения времени
let optimized = vtt::optimize_time_distribution(cues, &analysis);
```

### Генерация речи через OpenAI API

```rust
use crate::utils::tts::openai_tts;
use crate::utils::tts::types::TtsVoiceConfig;

// Конфигурация голоса
let voice_config = TtsVoiceConfig {
    model: "tts-1".to_string(),
    voice: "alloy".to_string(),
    speed: 1.0,
};

// Генерация речи
let (audio_data, text) = openai_tts::generate_tts(
    "your-api-key",
    "Текст для озвучивания",
    &voice_config
).await?;
```

### Обработка аудио

```rust
use crate::utils::tts::audio_processing;
use crate::utils::tts::audio_format;
use crate::utils::tts::types::AudioProcessingConfig;

// Декодирование аудиофайла
let (samples, sample_rate) = audio_format::decode_audio_file("input.mp3")?;

// Применение fade
let mut processed = samples.clone();
audio_processing::apply_fade(&mut processed, 20, sample_rate); // 20ms fade

// Нормализация
let normalized = audio_processing::normalize_peak(&processed, 0.8)?;

// Изменение длительности
let config = AudioProcessingConfig::default();
let (stretched, new_duration) = audio_processing::adjust_duration(
    &normalized,
    samples.len() as f32 / sample_rate as f32, // исходная длительность
    2.0, // целевая длительность (2 секунды)
    0.0, // без доп. времени
    sample_rate,
    &config
)?;

// Сохранение в WAV
audio_format::encode_wav(&stretched, sample_rate, "output.wav")?;
```

## Разработка и расширение

### Добавление нового голоса

Для добавления нового голоса, обновите список доступных голосов в `openai_tts.rs`:

```rust
// Пример добавления нового голоса
pub enum Voice {
    Alloy,
    Echo,
    Fable,
    Onyx,
    Nova,
    Shimmer,
    CustomVoice, // Новый голос
}

impl Voice {
    pub fn as_str(&self) -> &'static str {
        match self {
            Voice::Alloy => "alloy",
            Voice::Echo => "echo",
            // ...
            Voice::CustomVoice => "custom_voice", // ID нового голоса
        }
    }
}
```

### Добавление нового формата аудио

Для поддержки нового формата аудио, расширьте функцию `decode_audio_file` в `audio_format.rs`:

```rust
pub fn decode_audio_file<P: AsRef<Path>>(file_path: P) -> Result<(Vec<f32>, u32)> {
    // ...
    match extension.as_str() {
        "wav" => decode_wav_file(file_path),
        "mp3" | "m4a" | "aac" | "flac" | "ogg" => {
            // существующий код...
        },
        "new_format" => {
            // код для декодирования нового формата
        },
        _ => Err(TtsError::AudioProcessingError(format!("Неподдерживаемый формат аудио: {}", extension)))
    }
}
```

## Обработка ошибок

Система использует единый тип ошибок `TtsError`, определенный в `types.rs`. Это позволяет получать конкретную информацию о проблеме:

```rust
pub enum TtsError {
    VttParsingError(String),
    OpenAIApiError(String),
    AudioProcessingError(String),
    TimeStretchingError(String),
    WavEncodingError(hound::Error),
    WavDecodingError(hound::Error),
    IoError(std::io::Error),
    DemuxError(String),
    // ...
}
```

При обработке ошибок рекомендуется использовать match для определения конкретного типа ошибки:

```rust
match result {
    Ok(_) => println!("Успех"),
    Err(TtsError::OpenAIApiError(e)) => println!("Ошибка API: {}", e),
    Err(TtsError::VttParsingError(e)) => println!("Ошибка парсинга VTT: {}", e),
    Err(e) => println!("Другая ошибка: {}", e),
}
```

## Производительность и оптимизация

- **Кеширование** - используется для сохранения результатов TTS генерации, чтобы избежать повторных вызовов API
- **Многопоточность** - генерация фрагментов может выполняться параллельно
- **Блочная обработка** - аудио обрабатывается блоками для экономии памяти
- **Адаптивные алгоритмы** - система автоматически выбирает оптимальный алгоритм обработки аудио в зависимости от необходимых изменений

## Тестирование

Для каждого модуля разработаны модульные тесты, которые проверяют корректность работы отдельных функций:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_normalize_peak() {
        // тест нормализации...
    }
    
    #[test]
    fn test_apply_fade() {
        // тест применения fade...
    }
}
```

## Частые проблемы и решения

1. **Проблема**: OpenAI API возвращает ошибку аутентификации
   **Решение**: Проверьте правильность API-ключа и его токен доступа

2. **Проблема**: Время синхронизации сильно отличается от ожидаемого
   **Решение**: Используйте функцию `analyze_segments` для выявления проблемных сегментов

3. **Проблема**: Высокая нагрузка при обработке длинных аудио
   **Решение**: Разделите длинные аудио на части или используйте потоковую обработку

## Зависимости

- **rubato** - для высококачественного ресемплинга
- **hound** - для работы с WAV-файлами
- **symphonia** - для декодирования аудиоформатов
- **reqwest** - для HTTP-запросов к API
- **tokio** - для асинхронного выполнения
- **log** - для логирования

## Лицензия

Данная TTS система распространяется под лицензией [MIT](LICENSE). 