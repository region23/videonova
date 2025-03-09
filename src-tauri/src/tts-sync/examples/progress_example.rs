//! Пример использования системы прогресса и уведомлений
//! 
//! Этот пример демонстрирует, как использовать систему прогресса и уведомлений
//! при работе с библиотекой tts-sync.

use std::sync::Arc;
use tts_sync::{
    TtsSync, TtsSyncConfig, TtsModel, TtsVoice,
    progress::{ProgressReporter, DefaultProgressReporter},
    notification::{
        ConsoleProgressObserver, ProgressBarObserver, 
        FileProgressObserver, CompositeProgressObserver
    }
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Инициализируем логирование
    env_logger::init();
    
    // Получаем API ключ из переменной окружения
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set");
    
    // Пути к файлам
    let video_path = "path/to/original_video.mp4";
    let audio_path = "path/to/original_audio.mp3";
    let original_vtt_path = "path/to/original_subtitles.vtt";
    let translated_vtt_path = "path/to/translated_subtitles.vtt";
    let output_path = "path/to/output_video.mp4";
    
    println!("Пример 1: Использование функции-обертки с прогрессом");
    
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
    
    // Используем функцию-обертку с поддержкой прогресса
    let result = tts_sync::synchronize_tts_with_progress(
        video_path,
        audio_path,
        original_vtt_path,
        translated_vtt_path,
        output_path,
        &api_key,
        Box::new(reporter),
    ).await?;
    
    println!("Синхронизация завершена. Выходной файл: {}", result);
    
    println!("\nПример 2: Использование объекта TtsSync с настраиваемой конфигурацией");
    
    // Создаем конфигурацию
    let config = TtsSyncConfig {
        openai_api_key: api_key,
        tts_model: TtsModel::HighDefinition,
        tts_voice: TtsVoice::Nova,
        original_audio_volume: 0.15,
        tts_audio_volume: 1.0,
        ..TtsSyncConfig::default()
    };
    
    // Создаем новый репортер прогресса
    let reporter = DefaultProgressReporter::new();
    
    // Создаем объект TtsSync с репортером прогресса
    let mut tts_sync = TtsSync::with_progress_reporter(config, Box::new(reporter));
    
    // Добавляем наблюдатель для вывода в консоль
    tts_sync.add_observer(Box::new(ConsoleProgressObserver::with_prefix("[Custom] ")));
    
    // Добавляем наблюдатель для отображения прогресс-бара
    tts_sync.add_observer(Box::new(ProgressBarObserver::new(50)));
    
    // Запускаем процесс
    let result = tts_sync.process(
        video_path,
        audio_path,
        original_vtt_path,
        translated_vtt_path,
        "path/to/custom_output.mp4",
    ).await?;
    
    println!("Пользовательская синхронизация завершена. Выходной файл: {}", result);
    
    Ok(())
}
