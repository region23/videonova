use tauri::{Window, Manager, Emitter};
use serde_json::json;
use std::path::Path;
use crate::services::tts::fishspeech;

/// Check if a file exists and is accessible
#[tauri::command]
pub async fn check_file_exists_command(path: String) -> Result<bool, String> {
    Ok(Path::new(&path).exists())
}

/// Clean up temporary files after processing
#[tauri::command]
pub async fn cleanup_temp_files(final_video_path: String, output_dir: String) -> Result<(), String> {
    log::info!("Starting cleanup with final_video_path: {} and output_dir: {}", final_video_path, output_dir);

    // Ensure output_dir exists and is a directory
    let cleanup_dir = std::path::Path::new(&output_dir);
    if !cleanup_dir.exists() || !cleanup_dir.is_dir() {
        return Err(format!("Output directory does not exist or is not a directory: {}", output_dir));
    }

    // Get the filename from the final video path
    let final_video_name = std::path::Path::new(&final_video_path)
        .file_name()
        .ok_or("Failed to get video filename")?
        .to_str()
        .ok_or("Invalid video filename")?;

    // Construct the destination path in the output directory
    let destination_path = cleanup_dir.join(final_video_name);

    // Get the base filename (without extension and language suffix) from the final video
    let base_filename = std::path::Path::new(&final_video_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| {
            // Remove language suffix if present (e.g., "_ru" from "video_ru.mp4")
            if let Some(pos) = s.rfind('_') {
                &s[..pos]
            } else {
                s
            }
        })
        .unwrap_or("");

    log::info!("Base filename for cleanup: {}", base_filename);
    log::info!("Cleaning up in directory: {}", cleanup_dir.display());

    // Define and remove known temporary directories
    let temp_directories = ["tts"];
    for dir_name in temp_directories.iter() {
        let dir_path = cleanup_dir.join(dir_name);
        if dir_path.exists() && dir_path.is_dir() {
            log::info!("Removing directory: {}", dir_path.display());
            if let Err(e) = tokio::fs::remove_dir_all(&dir_path).await {
                log::warn!("Failed to remove temporary directory {}: {}", dir_path.display(), e);
            } else {
                log::info!("Successfully removed temporary directory: {}", dir_path.display());
            }
        }
    }

    // Remove temporary files in the output directory
    let mut entries = tokio::fs::read_dir(cleanup_dir).await
        .map_err(|e| format!("Failed to read output directory: {}", e))?;

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.is_file() {
            let file_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // Check if the file starts with base_filename and has one of our temp extensions
            let is_temp_file = file_name.starts_with(base_filename) && (
                file_name.ends_with("_audio.m4a") ||
                file_name.ends_with("_video.mp4") ||
                file_name.ends_with(".vtt") ||
                file_name.ends_with(".ass") ||
                file_name.ends_with("_tts.wav")
            );

            if is_temp_file {
                log::info!("Removing temporary file: {}", path.display());
                if let Err(e) = tokio::fs::remove_file(&path).await {
                    log::warn!("Failed to remove temporary file {}: {}", path.display(), e);
                } else {
                    log::info!("Successfully removed temporary file: {}", path.display());
                }
            } else {
                log::info!("Skipping non-temporary file: {}", file_name);
            }
        }
    }

    // Ensure the final video is in the correct location
    if final_video_path != destination_path.to_string_lossy() {
        log::info!("Moving final video from {} to {}", final_video_path, destination_path.display());
        if let Err(e) = tokio::fs::rename(&final_video_path, &destination_path).await {
            // If rename fails (possibly due to cross-device link), try copy and delete
            if let Err(copy_err) = tokio::fs::copy(&final_video_path, &destination_path).await {
                log::error!("Failed to copy video file: {}", copy_err);
                return Err(format!("Failed to move video file: {}", e));
            }
            if let Err(del_err) = tokio::fs::remove_file(&final_video_path).await {
                log::warn!("Failed to remove source video file after copying: {}", del_err);
            }
        }
        log::info!("Successfully moved final video to: {}", destination_path.display());
    } else {
        log::info!("Final video is already in the correct location");
    }
    
    Ok(())
}

/// Check if Fish Speech is installed
#[tauri::command]
pub async fn check_fish_speech_installed() -> Result<bool, String> {
    Ok(fishspeech::is_ready())
}

/// Check availability of YouTube and OpenAI services
/// 
/// @param isRetry - flag indicating whether this is a retry check (e.g., after enabling VPN)
/// @returns Check result with information about availability of each service
#[tauri::command]
pub async fn check_services_availability(
    isRetry: bool,
    window: Window
) -> Result<serde_json::Value, String> {
    // Report check start via event
    let _ = window.emit("services-check-started", json!({
        "is_retry": isRetry
    }));
    
    // Variables to store check results
    let mut youtube_available = false;
    let mut openai_available = false;
    
    // 1. Check YouTube availability
    let _ = window.emit("checking-youtube", {});
    
    // Check YouTube with API request
    match reqwest::Client::new()
        .get("https://www.youtube.com/")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(response) => {
            youtube_available = response.status().is_success();
            log::info!("YouTube availability: {}", youtube_available);
        }
        Err(e) => {
            log::error!("Error checking YouTube: {}", e);
            youtube_available = false;
        }
    }
    
    // Send YouTube check result
    let _ = window.emit("youtube-check-complete", youtube_available);
    
    // 2. Check OpenAI availability
    let _ = window.emit("checking-openai", {});
    
    // Check OpenAI with API request
    match reqwest::Client::new()
        .get("https://api.openai.com/v1/models")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(response) => {
            // We don't check status because without API key it will return 401,
            // but the fact that we got a response means the service is available
            openai_available = true;
            log::info!("OpenAI availability: {}", openai_available);
        }
        Err(e) => {
            log::error!("Error checking OpenAI: {}", e);
            openai_available = false;
        }
    }
    
    // Send OpenAI check result
    let _ = window.emit("openai-check-complete", openai_available);
    
    // Form final check result
    let vpn_required = !(youtube_available && openai_available);
    
    // Form message based on check results
    let message = if youtube_available && openai_available {
        "All services are available. You can start using the application.".to_string()
    } else if !youtube_available && !openai_available {
        "YouTube and OpenAI are blocked in your region. Please use a VPN to work with the application.".to_string()
    } else if !youtube_available {
        "YouTube is blocked in your region. VPN is required to download videos.".to_string()
    } else {
        "OpenAI is blocked in your region. VPN is required to generate speech.".to_string()
    };
    
    // Form final result
    let result = json!({
        "youtube_available": youtube_available,
        "openai_available": openai_available,
        "vpn_required": vpn_required,
        "message": message,
        "is_retry": isRetry
    });
    
    // Send event about check completion
    let _ = window.emit("services-check-completed", result.clone());
    
    Ok(result)
} 