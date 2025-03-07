use anyhow::{anyhow, Result};
use log::{debug, info, error};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::sync::mpsc;

/// Structure for holding merge progress information
#[derive(Clone, Serialize, Deserialize)]
pub struct MergeProgress {
    pub status: String,
    pub progress: f32,
}

/// Merge video, audio, and subtitles files using ffmpeg
pub async fn merge_files(
    video_path: &Path,
    translated_audio_path: &Path,
    original_vtt_path: &Path,
    translated_vtt_path: &Path,
    output_dir: &Path,
    progress_sender: Option<mpsc::Sender<MergeProgress>>,
) -> Result<PathBuf> {
    info!("Starting final merge process");
    
    if !video_path.exists() {
        return Err(anyhow!("Video file not found at {}", video_path.display()));
    }
    
    if !translated_audio_path.exists() {
        return Err(anyhow!("Translated audio file not found at {}", translated_audio_path.display()));
    }
    
    if !original_vtt_path.exists() {
        return Err(anyhow!("Original subtitles file not found at {}", original_vtt_path.display()));
    }
    
    if !translated_vtt_path.exists() {
        return Err(anyhow!("Translated subtitles file not found at {}", translated_vtt_path.display()));
    }
    
    // Create output directory if it doesn't exist
    if !output_dir.exists() {
        tokio::fs::create_dir_all(output_dir).await?;
    }
    
    // Report initial progress
    if let Some(sender) = &progress_sender {
        sender
            .send(MergeProgress {
                status: "Starting merge process".to_string(),
                progress: 0.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }
    
    // Extract base filename from video path
    let filename = video_path
        .file_stem()
        .ok_or_else(|| anyhow!("Failed to get file stem from video path"))?
        .to_string_lossy();
    
    // Create output file path
    let output_file = output_dir.join(format!("{}_translated.mp4", filename));
    
    // Update progress
    if let Some(sender) = &progress_sender {
        sender
            .send(MergeProgress {
                status: "Merging video with translated audio and subtitles".to_string(),
                progress: 20.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }
    
    // Get original audio track from video file
    // We'll extract metadata to understand what streams we have
    let ffprobe_output = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_streams",
            video_path.to_str().unwrap(),
        ])
        .output()?;
    
    if !ffprobe_output.status.success() {
        return Err(anyhow!("Failed to get video metadata"));
    }
    
    // Find indices for original audio and video tracks
    let json: serde_json::Value = serde_json::from_slice(&ffprobe_output.stdout)?;
    let streams = json.get("streams").and_then(|s| s.as_array()).ok_or_else(|| anyhow!("Invalid JSON structure"))?;
    
    let video_index = streams.iter()
        .position(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("video"))
        .ok_or_else(|| anyhow!("No video stream found"))?;
    
    let audio_index = streams.iter()
        .position(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("audio"))
        .ok_or_else(|| anyhow!("No audio stream found"))?;
    
    // Convert subtitle files to .ass format which has better styling support
    if let Some(sender) = &progress_sender {
        sender
            .send(MergeProgress {
                status: "Converting subtitles to .ass format".to_string(),
                progress: 40.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }
    
    let original_ass_path = output_dir.join(format!("{}_original.ass", filename));
    let translated_ass_path = output_dir.join(format!("{}_translated.ass", filename));
    
    // Convert original VTT to ASS
    let vtt_to_ass_orig = Command::new("ffmpeg")
        .args([
            "-i", original_vtt_path.to_str().unwrap(),
            "-y", original_ass_path.to_str().unwrap(),
        ])
        .status()?;
    
    if !vtt_to_ass_orig.success() {
        return Err(anyhow!("Failed to convert original subtitles to ASS format"));
    }
    
    // Convert translated VTT to ASS
    let vtt_to_ass_trans = Command::new("ffmpeg")
        .args([
            "-i", translated_vtt_path.to_str().unwrap(),
            "-y", translated_ass_path.to_str().unwrap(),
        ])
        .status()?;
    
    if !vtt_to_ass_trans.success() {
        return Err(anyhow!("Failed to convert translated subtitles to ASS format"));
    }
    
    // Update progress
    if let Some(sender) = &progress_sender {
        sender
            .send(MergeProgress {
                status: "Running final ffmpeg merge".to_string(),
                progress: 60.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }
    
    // Run ffmpeg to combine everything
    // -map 0:v     -> take video stream from first input
    // -map 1:a     -> take audio stream from second input (translated)
    // -map 0:a     -> take original audio stream
    // -c:v copy    -> copy video stream without re-encoding
    // -c:a:0 copy  -> copy first audio stream (translated) without re-encoding
    // -c:a:1 copy  -> copy second audio stream (original) without re-encoding
    // -metadata:s:a:0 -> set metadata for first audio stream
    // -metadata:s:a:1 -> set metadata for second audio stream
    let ffmpeg_args = [
        "-i", video_path.to_str().unwrap(),                       // Original video with audio
        "-i", translated_audio_path.to_str().unwrap(),            // Translated audio
        "-i", original_ass_path.to_str().unwrap(),                // Original subtitles
        "-i", translated_ass_path.to_str().unwrap(),              // Translated subtitles
        "-map", "0:v",                                            // Video from original
        "-map", "1:a",                                            // Audio from translated
        "-map", &format!("0:{}", audio_index),                    // Original audio
        "-map", "2",                                              // Original subtitles
        "-map", "3",                                              // Translated subtitles
        "-c:v", "copy",                                           // Copy video codec
        "-c:a", "copy",                                           // Copy audio codec
        "-c:s", "copy",                                           // Copy subtitle codec
        "-metadata:s:a:0", "title=Translated Audio",              // Label translated audio
        "-metadata:s:a:1", "title=Original Audio",                // Label original audio
        "-metadata:s:s:0", "title=Original Subtitles",            // Label original subs
        "-metadata:s:s:1", "title=Translated Subtitles",          // Label translated subs
        "-disposition:a:0", "default",                            // Set translated audio as default
        "-disposition:s:1", "default",                            // Set translated subs as default
        "-y", output_file.to_str().unwrap()                       // Output file
    ];
    
    let status = Command::new("ffmpeg")
        .args(&ffmpeg_args)
        .status()?;
    
    if !status.success() {
        return Err(anyhow!("Failed to merge files with ffmpeg"));
    }
    
    // Update progress to complete
    if let Some(sender) = &progress_sender {
        sender
            .send(MergeProgress {
                status: "Merge complete".to_string(),
                progress: 100.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }
    
    // Attempt to delete temporary ASS files
    let _ = std::fs::remove_file(&original_ass_path);
    let _ = std::fs::remove_file(&translated_ass_path);
    
    info!("Merge complete. Output file: {}", output_file.display());
    
    Ok(output_file)
} 