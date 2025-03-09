# TTS-Sync с системой прогресса и уведомлений

Библиотека TTS-Sync предоставляет полный набор инструментов для создания синхронизированной озвучки видео на основе переведенных субтитров с использованием OpenAI TTS API. Теперь библиотека включает систему прогресса и уведомлений, которая позволяет асинхронно отслеживать ход выполнения длительных операций.

## Возможности

- Парсинг и анализ VTT файлов
- Оптимизация субтитров для TTS
- Генерация речи с использованием OpenAI TTS API
- Синхронизация аудио с видео
- **Новое**: Асинхронное отслеживание прогресса выполнения операций
- **Новое**: Различные способы уведомления о прогрессе (консоль, файл, прогресс-бар и др.)

## Установка

Добавьте библиотеку в зависимости вашего проекта:

```toml
[dependencies]
tts-sync = "0.2.0"
```

## Использование

### Базовое использование (без отслеживания прогресса)

```rust
use tts_sync::synchronize_tts;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set");
    
    let result = synchronize_tts(
        "video.mp4",
        "audio.mp3",
        "original.vtt",
        "translated.vtt",
        "output.mp4",
        &api_key,
    ).await?;
    
    println!("Синхронизация завершена. Выходной файл: {}", result);
    
    Ok(())
}
```

### Использование с отслеживанием прогресса

```rust
use tts_sync::{
    synchronize_tts_with_progress,
    progress::DefaultProgressReporter,
    notification::ConsoleProgressObserver
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set");
    
    // Создаем репортер прогресса
    let mut reporter = DefaultProgressReporter::new();
    
    // Добавляем наблюдатель для вывода в консоль
    reporter.add_observer(Box::new(ConsoleProgressObserver::new()));
    
    // Используем функцию-обертку с поддержкой прогресса
    let result = synchronize_tts_with_progress(
        "video.mp4",
        "audio.mp3",
        "original.vtt",
        "translated.vtt",
        "output.mp4",
        &api_key,
        Box::new(reporter),
    ).await?;
    
    println!("Синхронизация завершена. Выходной файл: {}", result);
    
    Ok(())
}
```

### Использование с настраиваемой конфигурацией

```rust
use tts_sync::{
    TtsSync, TtsSyncConfig, TtsModel, TtsVoice,
    progress::DefaultProgressReporter,
    notification::{ConsoleProgressObserver, ProgressBarObserver}
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set");
    
    // Создаем конфигурацию
    let config = TtsSyncConfig {
        openai_api_key: api_key,
        tts_model: TtsModel::HighDefinition,
        tts_voice: TtsVoice::Nova,
        original_audio_volume: 0.15,
        tts_audio_volume: 1.0,
        ..TtsSyncConfig::default()
    };
    
    // Создаем репортер прогресса
    let mut reporter = DefaultProgressReporter::new();
    
    // Создаем объект TtsSync с репортером прогресса
    let mut tts_sync = TtsSync::with_progress_reporter(config, Box::new(reporter));
    
    // Добавляем наблюдатель для вывода в консоль
    tts_sync.add_observer(Box::new(ConsoleProgressObserver::new()));
    
    // Добавляем наблюдатель для отображения прогресс-бара
    tts_sync.add_observer(Box::new(ProgressBarObserver::new(50)));
    
    // Запускаем процесс
    let result = tts_sync.process(
        "video.mp4",
        "audio.mp3",
        "original.vtt",
        "translated.vtt",
        "output.mp4",
    ).await?;
    
    println!("Синхронизация завершена. Выходной файл: {}", result);
    
    Ok(())
}
```

## Система прогресса и уведомлений

### Доступные наблюдатели

Библиотека предоставляет несколько типов наблюдателей для отслеживания прогресса:

- `ConsoleProgressObserver` - вывод информации о прогрессе в консоль
- `ProgressBarObserver` - отображение прогресс-бара в консоли
- `FileProgressObserver` - запись информации о прогрессе в файл
- `MemoryProgressObserver` - сохранение информации о прогрессе в памяти
- `ChannelProgressObserver` - отправка информации о прогрессе через канал
- `CallbackProgressObserver` - вызов функции обратного вызова при обновлении прогресса
- `CompositeProgressObserver` - комбинирование нескольких наблюдателей

### Комбинирование наблюдателей

```rust
use tts_sync::{
    progress::DefaultProgressReporter,
    notification::{
        ConsoleProgressObserver, ProgressBarObserver, 
        FileProgressObserver, CompositeProgressObserver
    }
};

// Создаем репортер прогресса
let mut reporter = DefaultProgressReporter::new();

// Создаем комбинированный наблюдатель
let mut composite_observer = CompositeProgressObserver::new();

// Добавляем наблюдатель для вывода в консоль
composite_observer.add_observer(Box::new(ConsoleProgressObserver::new()));

// Добавляем наблюдатель для отображения прогресс-бара
composite_observer.add_observer(Box::new(ProgressBarObserver::new(50)));

// Добавляем наблюдатель для записи в файл
composite_observer.add_observer(Box::new(FileProgressObserver::new("progress.log")));

// Добавляем комбинированный наблюдатель к репортеру
reporter.add_observer(Box::new(composite_observer));
```

### Создание собственного наблюдателя

```rust
use tts_sync::progress::{ProgressObserver, ProgressInfo};

// Собственный наблюдатель
struct MyCustomObserver;

impl ProgressObserver for MyCustomObserver {
    fn on_progress_update(&self, progress: ProgressInfo) {
        println!(
            "Мой наблюдатель: Шаг '{}' - {}% выполнено, общий прогресс: {}%",
            progress.step,
            progress.step_progress,
            progress.total_progress
        );
        
        if let Some(details) = progress.details {
            println!("Детали: {}", details);
        }
    }
}

// Использование собственного наблюдателя
let mut tts_sync = TtsSync::default();
tts_sync.add_observer(Box::new(MyCustomObserver));
```

### Асинхронные уведомления

```rust
use tts_sync::progress::AsyncProgressReporter;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Создаем асинхронный репортер прогресса
    let (reporter, mut rx) = AsyncProgressReporter::new();
    
    // Запускаем обработчик уведомлений в отдельной задаче
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            println!(
                "Асинхронное уведомление: Шаг '{}' - {}% выполнено, общий прогресс: {}%",
                progress.step,
                progress.step_progress,
                progress.total_progress
            );
        }
    });
    
    // Используем асинхронный репортер
    let tts_sync = TtsSync::with_progress_reporter(TtsSyncConfig::default(), Box::new(reporter));
    
    // Запускаем процесс
    let result = tts_sync.process(
        "video.mp4",
        "audio.mp3",
        "original.vtt",
        "translated.vtt",
        "output.mp4",
    ).await?;
    
    println!("Синхронизация завершена. Выходной файл: {}", result);
    
    Ok(())
}
```

## Этапы процесса и их весовые коэффициенты

Для корректного расчета общего прогресса выполнения, определены следующие этапы процесса и их весовые коэффициенты:

1. `SubtitleParsing` - Парсинг и анализ субтитров - 5%
2. `TimingAnalysis` - Анализ временных меток - 5%
3. `SubtitleOptimization` - Оптимизация субтитров для TTS - 10%
4. `SpeechGeneration` - Генерация речи с использованием OpenAI API - 60%
5. `AudioVideoSync` - Синхронизация аудио с видео - 20%

## Требования

- Rust 1.56 или выше
- FFmpeg (должен быть установлен и доступен в PATH)
- OpenAI API ключ

## Лицензия

MIT
