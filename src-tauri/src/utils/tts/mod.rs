//! # TTS (Text-to-Speech) модуль
//! 
//! Модуль для генерации и обработки речи из текста.
//! Включает в себя интеграцию с OpenAI TTS API, обработку аудио,
//! управление темпом речи и синхронизацию с субтитрами.

pub mod types;
pub mod soundtouch;
pub mod vtt;
pub mod openai_tts;
pub mod audio_format;
pub mod audio_processing;
pub mod synchronizer;
pub mod demucs;
pub mod analysis;
// Устаревший модуль tts.rs удален, так как его функциональность теперь распределена по модульной архитектуре

// Публично экспортируем основные типы и API для удобства использования
pub use types::{
    TtsError, Result, SubtitleCue, AudioFragment, ProgressUpdate, 
    TtsVoiceConfig, AudioProcessingConfig, SyncConfig
};
pub use synchronizer::synchronize_tts;
pub use demucs::separate_audio;
pub use analysis::{SegmentAnalysisConfig, SegmentAnalysisResult, analyze_segments};

#[cfg(test)]
mod tests {
    mod test_vtt_parser;
    mod test_openai_tts;
    mod test_analysis;
    mod test_synchronizer;
    mod test_audio_processing;
} 