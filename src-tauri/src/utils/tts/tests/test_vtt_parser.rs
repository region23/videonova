//! Тест для модуля разбора VTT субтитров

use std::fs::File;
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

use crate::utils::tts::vtt;
use crate::utils::tts::types::{SegmentAnalysisConfig, SubtitleCue};

fn create_test_vtt() -> (tempfile::TempDir, String) {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.vtt");
    let file_path_str = file_path.to_str().unwrap().to_string();
    
    let mut file = File::create(&file_path).unwrap();
    file.write_all(r#"WEBVTT

1
00:00:01.000 --> 00:00:03.000
Это первая строка субтитров.

2
00:00:03.500 --> 00:00:05.000
Это вторая строка.

3
00:00:05.500 --> 00:00:10.000
Это длинная строка с большим количеством текста, которая потребует более высокой скорости чтения и может вызвать проблемы с синхронизацией, если ее не обработать правильно.
"#.as_bytes()).unwrap();
    
    (dir, file_path_str)
}

#[test]
fn test_parse_vtt() {
    let (dir, file_path) = create_test_vtt();
    
    let cues = vtt::parse_vtt(&file_path).unwrap();
    
    assert_eq!(cues.len(), 3);
    assert_eq!(cues[0].start, 1.0);
    assert_eq!(cues[0].end, 3.0);
    assert_eq!(cues[0].text, "Это первая строка субтитров.");
    
    assert_eq!(cues[1].start, 3.5);
    assert_eq!(cues[1].end, 5.0);
    assert_eq!(cues[1].text, "Это вторая строка.");
    
    assert_eq!(cues[2].start, 5.5);
    assert_eq!(cues[2].end, 10.0);
    assert!(cues[2].text.contains("длинная строка"));
    
    // Предотвращаем преждевременное удаление временных файлов
    drop(dir);
}

#[test]
fn test_analyze_segments() {
    let cues = vec![
        SubtitleCue {
            start: 0.0,
            end: 3.0,
            text: "Нормальная скорость речи в этом субтитре.".to_string(),
        },
        SubtitleCue {
            start: 3.0,
            end: 4.0,
            text: "Этот субтитр содержит слишком много слов для такого короткого интервала времени и явно будет проблемным.".to_string(),
        },
        SubtitleCue {
            start: 4.0,
            end: 4.05,
            text: "Слишком короткий субтитр.".to_string(),
        },
    ];
    
    let config = SegmentAnalysisConfig::default();
    let analysis = vtt::analyze_segments(&cues, &config);
    
    assert_eq!(analysis.len(), 3);
    
    // Первый субтитр должен быть нормальным
    assert_eq!(analysis[0].severity, 0);
    
    // Второй субтитр должен быть проблемным
    assert!(analysis[1].severity > 0);
    assert!(analysis[1].words_per_second > config.max_words_per_second);
    
    // Третий субтитр должен быть критически проблемным из-за короткой длительности
    assert_eq!(analysis[2].severity, 10);
}

#[test]
fn test_optimize_time_distribution() {
    let cues = vec![
        SubtitleCue {
            start: 0.0,
            end: 3.0,
            text: "У этого субтитра много свободного времени.".to_string(),
        },
        SubtitleCue {
            start: 3.0,
            end: 4.0,
            text: "Этот субтитр содержит слишком много слов для такого короткого интервала времени и явно будет проблемным.".to_string(),
        },
        SubtitleCue {
            start: 4.0,
            end: 7.0,
            text: "Еще один субтитр с достаточным количеством времени.".to_string(),
        },
    ];
    
    let config = SegmentAnalysisConfig::default();
    let analysis = vtt::analyze_segments(&cues, &config);
    let optimized = vtt::optimize_time_distribution(cues.clone(), &analysis);
    
    // Количество субтитров должно остаться тем же
    assert_eq!(optimized.len(), cues.len());
    
    // Для проблемного субтитра должно быть заимствовано время
    let original_duration = cues[1].end - cues[1].start;
    let new_duration = optimized[1].end - optimized[1].start;
    
    // Новая длительность должна быть больше
    assert!(new_duration > original_duration);
} 