use crate::utils::tts::analysis::{analyze_segments, SegmentAnalysisConfig};
use crate::utils::tts::types::SubtitleCue;

#[test]
fn test_analyze_segments_normal_speed() {
    // Setup test data - normal speed segments
    let cues = vec![
        SubtitleCue {
            start: 0.0,
            end: 5.0,
            text: "This is a test with five words".to_string(),
        }
    ];
    
    let config = SegmentAnalysisConfig::default();
    let results = analyze_segments(&cues, &config);
    
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_count, 7); // "This", "is", "a", "test", "with", "five", "words"
    assert_eq!(results[0].duration, 5.0);
    assert!(results[0].words_per_second < config.max_words_per_second);
    assert_eq!(results[0].severity, 0); // No problem expected
}

#[test]
fn test_analyze_segments_fast_speech() {
    // Setup test data - fast speech that exceeds limits
    let cues = vec![
        SubtitleCue {
            start: 0.0,
            end: 1.0,
            text: "This is a very fast speech segment with many words".to_string(),
        }
    ];
    
    let config = SegmentAnalysisConfig::default();
    let results = analyze_segments(&cues, &config);
    
    assert_eq!(results.len(), 1);
    assert!(results[0].words_per_second > config.max_words_per_second);
    assert!(results[0].severity > 0); // Should detect problem
}

#[test]
fn test_analyze_segments_zero_duration() {
    // Test with zero duration segment
    let cues = vec![
        SubtitleCue {
            start: 10.0,
            end: 10.0, // Zero duration
            text: "This should be handled gracefully".to_string(),
        }
    ];
    
    let config = SegmentAnalysisConfig::default();
    let results = analyze_segments(&cues, &config);
    
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].words_per_second, 0.0); // Should handle division by zero
    assert_eq!(results[0].required_speed_factor, 0.0);
}

#[test]
fn test_analyze_segments_empty_text() {
    // Test with empty text
    let cues = vec![
        SubtitleCue {
            start: 0.0,
            end: 3.0,
            text: "".to_string(), // Empty text
        }
    ];
    
    let config = SegmentAnalysisConfig::default();
    let results = analyze_segments(&cues, &config);
    
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].word_count, 0);
    assert_eq!(results[0].words_per_second, 0.0);
    assert_eq!(results[0].severity, 0); // No problem with empty text
}

#[test]
fn test_analyze_segments_multiple_cues() {
    // Test with multiple cues with varying speech rates
    let cues = vec![
        SubtitleCue {
            start: 0.0,
            end: 5.0,
            text: "This is a normal speed segment".to_string(),
        },
        SubtitleCue {
            start: 5.0,
            end: 6.0,
            text: "This is very fast and should be detected".to_string(),
        },
        SubtitleCue {
            start: 6.0,
            end: 15.0,
            text: "This is a slow segment with few words".to_string(),
        }
    ];
    
    let config = SegmentAnalysisConfig::default();
    let results = analyze_segments(&cues, &config);
    
    assert_eq!(results.len(), 3);
    
    // First segment should be normal
    assert!(results[0].words_per_second < config.max_words_per_second);
    assert_eq!(results[0].severity, 0);
    
    // Second segment should be too fast
    assert!(results[1].words_per_second > config.max_words_per_second);
    assert!(results[1].severity > 0);
    
    // Third segment should be normal/slow
    assert!(results[2].words_per_second < config.max_words_per_second);
    assert_eq!(results[2].severity, 0);
}

#[test]
fn test_custom_analysis_config() {
    // Test with a custom config with stricter limits
    let cues = vec![
        SubtitleCue {
            start: 0.0,
            end: 5.0,
            text: "This would be normal with default settings".to_string(),
        }
    ];
    
    // Custom config with stricter limits
    let custom_config = SegmentAnalysisConfig {
        max_words_per_second: 1.0, // Much stricter than default
        max_speed_factor: 1.5,
    };
    
    let results = analyze_segments(&cues, &custom_config);
    
    assert_eq!(results.len(), 1);
    
    // With stricter config, this should now be flagged as too fast
    assert!(results[0].words_per_second > custom_config.max_words_per_second);
    assert!(results[0].severity > 0);
} 