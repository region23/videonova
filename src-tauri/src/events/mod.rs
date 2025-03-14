// Events module
// Contains event handling and emitting logic

use tauri::{Emitter, WebviewWindow, Window};
use log::{info, error};
use serde::Serialize;
use serde_json::json;

use crate::errors::AppError;

/// Emit an event to the frontend
pub fn emit_event<T: Serialize + Clone>(window: &WebviewWindow, event_name: &str, payload: T) {
    match window.emit(event_name, payload) {
        Ok(_) => info!("Emitted event: {}", event_name),
        Err(e) => error!("Failed to emit event {}: {}", event_name, e),
    }
}

/// Register event listeners
pub fn register_listeners(window: &WebviewWindow) {
    // Register application event listeners here
    info!("Registered event listeners");
}

pub fn emit_error(window: &Window, error: &AppError) {
    window.emit(
        "error",
        json!({
            "message": error.to_string(),
            "type": match error {
                AppError::ConfigurationError(_) => "configuration",
                AppError::AudioProcessingError(_) => "audio",
                AppError::ApiError(_) => "api",
                AppError::IoError(_) => "io",
                AppError::InstallationError(_) => "installation",
                AppError::Unknown(_) => "unknown",
                AppError::Other(_) => "other",
                AppError::SerializationError(_) => "serialization",
                AppError::AnyhowError(_) => "other",
            }
        }),
    ).unwrap_or_else(|e| {
        error!("Failed to emit error event: {}", e);
    });
}

pub fn emit_progress<T: serde::Serialize + Clone>(window: &Window, event: &str, progress: T) {
    window.emit(event, progress).unwrap_or_else(|e| {
        error!("Failed to emit progress event: {}", e);
    });
}

// Event handler functions will be implemented as needed 