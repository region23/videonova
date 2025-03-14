// Services module
// Contains business logic separated by domain areas

pub mod tts;       // Text-to-Speech services
pub mod video;     // Video processing services
pub mod transcription; // Audio transcription services
pub mod translation;   // Translation services
pub mod audio;     // Audio processing services
pub mod merge;     // Audio/video merge services

// Реэкспортируем common из utils для обратной совместимости
pub use crate::utils::common;

// Other service modules will be added as needed:
// - video:        Video processing services
// - transcription: Audio transcription services
// - translation:   Translation services 