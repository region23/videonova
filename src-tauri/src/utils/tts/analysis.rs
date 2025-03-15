//! # Segment Analysis for TTS
//! 
//! This module provides functionality for analyzing speech segments in subtitles
//! to identify potential issues with speech rate and timing.
//! 
//! ## Key features:
//! 
//! * Calculate words per second for each subtitle segment
//! * Identify segments that may require time-stretching
//! * Assign severity ratings to problematic segments
//! * Provide recommendations for time-stretching parameters
//! 
//! ## Usage example:
//! 
//! ```rust
//! use crate::utils::tts::analysis::{analyze_segments, SegmentAnalysisConfig};
//! use crate::utils::tts::types::SubtitleCue;
//! 
//! // Create some subtitle cues
//! let cues = vec![
//!     SubtitleCue {
//!         start: 0.0,
//!         end: 5.0,
//!         text: "This is a test subtitle".to_string(),
//!     }
//! ];
//! 
//! // Analyze the segments
//! let config = SegmentAnalysisConfig::default();
//! let analysis = analyze_segments(&cues, &config);
//! 
//! // Use the analysis results
//! for result in analysis {
//!     if result.severity > 5 {
//!         println!("Segment {} needs attention: {:.2} words/sec", 
//!                  result.index, result.words_per_second);
//!     }
//! }
//! ```

use crate::utils::tts::types::SubtitleCue;

/// Configuration for subtitle segment analysis.
/// 
/// This struct defines parameters that determine what speech rates
/// are considered acceptable and how to evaluate segments.
#[derive(Debug, Clone)]
pub struct SegmentAnalysisConfig {
    /// Maximum comfortable words per second for speech.
    /// Speech rates above this threshold may require time-stretching.
    /// 
    /// The default value is 2.5 words per second, which is considered
    /// a comfortable pace for most listeners.
    pub max_words_per_second: f32,
    
    /// Maximum speed factor that can be applied while maintaining intelligible speech.
    /// Used to calculate severity of timing issues.
    /// 
    /// This value is used to normalize the severity rating on a scale of 0-10.
    /// Higher values allow for more aggressive time-stretching before considering
    /// a segment problematic.
    pub max_speed_factor: f32,
}

impl Default for SegmentAnalysisConfig {
    fn default() -> Self {
        Self {
            max_words_per_second: 2.5,
            max_speed_factor: 1.8,
        }
    }
}

/// Result of analyzing a single subtitle segment.
/// 
/// Contains detailed information about the speech rate, severity of timing issues,
/// and recommended adjustments for the segment.
#[derive(Debug)]
pub struct SegmentAnalysisResult {
    /// Zero-based index of the segment in the original cues array
    pub index: usize,
    
    /// Number of words in the segment, calculated by splitting text on whitespace
    pub word_count: usize,
    
    /// Duration of the segment in seconds (end - start)
    pub duration: f32,
    
    /// Words per second rate for this segment
    /// 
    /// Calculated as word_count / duration. A value above max_words_per_second
    /// indicates the segment may be too fast for comfortable listening.
    pub words_per_second: f32,
    
    /// Severity of timing issues on a scale of 0-10
    /// 
    /// 0: No timing issue (words_per_second <= max_words_per_second)
    /// 1-3: Minor timing issue
    /// 4-7: Moderate timing issue
    /// 8-10: Severe timing issue
    pub severity: u8,
    
    /// Recommended speed factor for time-stretching
    /// 
    /// Values greater than 1.0 indicate the segment should be slowed down.
    /// The higher this value, the more the segment needs to be stretched.
    /// A value of 0.0 means no adjustment is needed.
    pub required_speed_factor: f32,
}

/// Analyzes subtitle segments to identify potential speech rate issues.
/// 
/// This function takes an array of subtitle cues and a configuration object,
/// and returns analysis results for each segment. The results can be used to
/// determine which segments need time-stretching and by how much.
/// 
/// # Arguments
/// 
/// * `cues` - Array of subtitle cues to analyze
/// * `config` - Configuration parameters for the analysis
/// 
/// # Returns
/// 
/// A vector of analysis results, one for each input cue in the same order
/// 
/// # Examples
/// 
/// ```
/// use crate::utils::tts::analysis::{analyze_segments, SegmentAnalysisConfig};
/// use crate::utils::tts::types::SubtitleCue;
/// 
/// let cues = vec![
///     SubtitleCue {
///         start: 0.0,
///         end: 2.0,
///         text: "This is a fast segment".to_string(),
///     }
/// ];
/// 
/// let config = SegmentAnalysisConfig::default();
/// let results = analyze_segments(&cues, &config);
/// 
/// // Check if the segment needs time-stretching
/// if results[0].severity > 0 {
///     println!("Segment needs to be slowed down by a factor of {:.2}", 
///              results[0].required_speed_factor);
/// }
/// ```
pub fn analyze_segments(cues: &[SubtitleCue], config: &SegmentAnalysisConfig) -> Vec<SegmentAnalysisResult> {
    let mut results = Vec::with_capacity(cues.len());
    
    for (i, cue) in cues.iter().enumerate() {
        let duration = cue.end - cue.start;
        let word_count = cue.text.split_whitespace().count();
        
        // Избегаем деления на ноль
        let words_per_second = if duration > 0.0 { word_count as f32 / duration } else { 0.0 };
        
        // Вычисляем, насколько скорость превышает максимально комфортную
        let required_speed_factor = if words_per_second > 0.0 { 
            words_per_second / config.max_words_per_second 
        } else { 
            0.0 
        };
        
        // Определяем критичность проблемы от 0 до 10
        let severity = if required_speed_factor <= 1.0 {
            // Если скорость ниже максимальной комфортной, проблемы нет
            0
        } else {
            // Линейно масштабируем от 1 до 10 по превышению max_speed_factor
            let severity_factor = (required_speed_factor - 1.0) / (config.max_speed_factor - 1.0);
            (severity_factor * 10.0).min(10.0) as u8
        };
        
        results.push(SegmentAnalysisResult {
            index: i,
            word_count,
            duration,
            words_per_second,
            severity,
            required_speed_factor,
        });
    }
    
    results
} 