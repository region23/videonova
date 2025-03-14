// Re-export commands from individual modules
pub mod tts_commands;
pub mod video_commands;
pub mod transcription_commands;
pub mod translation_commands;
pub mod utility_commands;
pub mod speech_commands;

// Re-export functions from modules
pub use tts_commands::*;
pub use video_commands::*;
pub use transcription_commands::*;
pub use translation_commands::*;
pub use utility_commands::*;
pub use speech_commands::*;

// We've moved all necessary functions from commands_root.rs to our modules
// No need to re-export anything from commands_root.rs anymore 