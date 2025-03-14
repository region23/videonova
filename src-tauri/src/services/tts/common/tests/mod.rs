use std::path::PathBuf;
use std::time::Duration;
use super::*;

#[test]
fn test_audio_fragment_processing() {
    let mut fragment = fragments::AudioFragment::new(
        0,
        Duration::from_secs(0),
        Duration::from_secs(1),
        vec![1.0; 44100],
        44100,
    );

    let config = fragments::FragmentProcessingConfig::default();
    fragments::process_fragment(&mut fragment, &config).unwrap();

    // Проверяем fade-in
    assert!(fragment.samples[0] < 0.1);
    // Проверяем fade-out
    assert!(fragment.samples[fragment.samples.len() - 1] < 0.1);
}

#[test]
fn test_fragment_merging() {
    let mut fragments = vec![
        fragments::AudioFragment::new(
            0,
            Duration::from_secs(0),
            Duration::from_secs(1),
            vec![1.0; 44100],
            44100,
        ),
        fragments::AudioFragment::new(
            1,
            Duration::from_secs(1),
            Duration::from_secs(2),
            vec![1.0; 44100],
            44100,
        ),
    ];

    let config = fragments::FragmentProcessingConfig::default();
    
    // Тест простого слияния
    let result = fragments::merge_fragments(&mut fragments.clone(), &config).unwrap();
    assert_eq!(result.len(), 88200); // 2 секунды при 44100 Hz

    // Тест слияния с кроссфейдом
    let result = fragments::merge_fragments_with_crossfade(&mut fragments, &config, 0.1).unwrap();
    assert_eq!(result.len(), 88200 - 4410); // 2 секунды - 0.1 секунда кроссфейда
}

#[test]
fn test_vtt_parsing() {
    let vtt_content = r#"WEBVTT

00:00:00.000 --> 00:00:05.000
First subtitle

00:00:05.000 --> 00:00:10.000
Second subtitle
"#;

    let temp_dir = tempfile::tempdir().unwrap();
    let vtt_path = temp_dir.path().join("test.vtt");
    std::fs::write(&vtt_path, vtt_content).unwrap();

    let result = subtitles::parse_vtt(&vtt_path).unwrap();
    assert_eq!(result.cues.len(), 2);
    assert_eq!(result.cues[0].text, "First subtitle");
    assert_eq!(result.cues[1].text, "Second subtitle");
    assert_eq!(result.duration, Duration::from_secs(10));
}

#[tokio::test]
async fn test_demucs_progress_parsing() {
    let test_lines = vec![
        ("Loading model...", Some(demucs::DemucsProgress::LoadingModel)),
        ("Converting to mp3...", Some(demucs::DemucsProgress::Converting)),
        ("progress: 45.5%", Some(demucs::DemucsProgress::Processing { progress: 0.455 })),
        ("random text", None),
    ];

    for (line, expected) in test_lines {
        let result = demucs::parse_demucs_progress(line);
        assert_eq!(format!("{:?}", result), format!("{:?}", expected));
    }
}

#[test]
fn test_audio_config() {
    let config = AudioProcessingConfig::default();
    assert_eq!(config.audio.target_peak_level, -14.0);
    assert_eq!(config.fragments.fade_in, 0.02);
    assert_eq!(config.fragments.fade_out, 0.02);
    assert!(config.demucs.use_gpu);
} 