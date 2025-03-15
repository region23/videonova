use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::utils::tts::synchronizer::synchronize_tts;
use crate::utils::tts::types::{
    SubtitleCue, ProgressUpdate, TtsVoiceConfig, 
    AudioProcessingConfig, SyncConfig
};

/// Тест для проверки обновлений прогресса в synchronizer
/// Этот тест не проверяет полную функциональность,
/// а только получение обновлений прогресса
#[tokio::test]
async fn test_progress_updates() {
    // Создаем временный VTT-файл для теста
    let temp_dir = tempfile::tempdir().unwrap();
    let vtt_path = temp_dir.path().join("test.vtt");
    let output_path = temp_dir.path().join("test_output.wav");
    
    // Тестовое содержимое VTT-файла
    let vtt_content = r#"WEBVTT

00:00:00.000 --> 00:00:05.000
Тестовая фраза для синхронизации
"#;
    
    // Записываем VTT-файл
    fs::write(&vtt_path, vtt_content).await.unwrap();
    
    // Создаем канал для получения обновлений прогресса
    let (tx, mut rx) = mpsc::channel(100);
    
    // Счетчик обновлений прогресса по типам
    let started_count = Arc::new(AtomicUsize::new(0));
    let parsing_count = Arc::new(AtomicUsize::new(0));
    let generation_count = Arc::new(AtomicUsize::new(0));
    
    // Клонируем счетчики для использования в task
    let started_count_clone = started_count.clone();
    let parsing_count_clone = parsing_count.clone();
    let generation_count_clone = generation_count.clone();
    
    // Создаем фоновую задачу для получения и подсчета обновлений прогресса
    let progress_task = tokio::spawn(async move {
        // Получаем обновления, пока канал открыт
        while let Some(update) = rx.recv().await {
            match update {
                ProgressUpdate::Started => {
                    started_count_clone.fetch_add(1, Ordering::SeqCst);
                },
                ProgressUpdate::ParsingVTT => {
                    parsing_count_clone.fetch_add(1, Ordering::SeqCst);
                },
                ProgressUpdate::TTSGeneration { .. } => {
                    generation_count_clone.fetch_add(1, Ordering::SeqCst);
                },
                _ => {}
            }
        }
    });
    
    // Создаем конфигурацию TTS с путем к тестовому VTT-файлу и получателем прогресса
    let config = SyncConfig {
        vtt_path: vtt_path.to_str().unwrap_or(""),
        output_wav: output_path.clone(),
        api_key: "фейковый_ключ_для_теста",  // Этот ключ не должен использоваться, так как запрос к API не должен выполняться
        tts_config: TtsVoiceConfig::default(),
        audio_config: AudioProcessingConfig::default(),
        original_audio_path: None,
        progress_sender: Some(tx),
    };
    
    // Проверка функции synchronize_tts - ожидаем ошибку, так как API-ключ недействителен,
    // но должны увидеть обновления прогресса перед ошибкой
    let _ = synchronize_tts(config).await;
    
    // Закрываем канал, чтобы task завершилась
    progress_task.abort();
    let _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Проверяем, что получили хотя бы одно обновление Started и ParsingVTT 
    // Ошибка должна произойти до генерации TTS, поэтому TTSGeneration может не быть
    assert!(started_count.load(Ordering::SeqCst) > 0, "Должно быть хотя бы одно обновление Started");
    assert!(parsing_count.load(Ordering::SeqCst) > 0, "Должно быть хотя бы одно обновление ParsingVTT");
}

#[test]
fn test_parse_subtitles() {
    // Тестовые данные
    let vtt_content = r#"WEBVTT

00:00:00.000 --> 00:00:05.000
First subtitle

00:00:06.000 --> 00:00:10.000
Second subtitle
"#;

    // Создаем временный файл с тестовыми данными
    let temp_dir = tempfile::tempdir().unwrap();
    let vtt_path = temp_dir.path().join("test.vtt");
    std::fs::write(&vtt_path, vtt_content).unwrap();
    
    // Парсим субтитры
    let cues = crate::utils::tts::vtt::parse_vtt(vtt_path.to_str().unwrap()).unwrap();
    
    // Проверяем результаты
    assert_eq!(cues.len(), 2);
    assert_eq!(cues[0].text, "First subtitle");
    assert_eq!(cues[0].start, 0.0);
    assert_eq!(cues[0].end, 5.0);
    
    assert_eq!(cues[1].text, "Second subtitle");
    assert_eq!(cues[1].start, 6.0);
    assert_eq!(cues[1].end, 10.0);
}

// TODO: Добавить интеграционные тесты, которые используют фиктивную реализацию API
// и проверяют весь процесс синхронизации 