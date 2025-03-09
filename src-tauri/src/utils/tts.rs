use anyhow::{anyhow, Result};
use reqwest::{Client, header};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::sync::mpsc;
use regex::Regex;
use log::info;
use crate::utils::common::check_file_exists_and_valid;

/// Structure for holding TTS progress information
#[derive(Clone, Serialize, Deserialize)]
pub struct TTSProgress {
    pub status: String,
    pub progress: f32,
    pub current_segment: usize,
    pub total_segments: usize,
}

/// Structure for holding a subtitle segment
#[derive(Debug)]
pub struct SubtitleSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub index: usize,
}

/// Convert a VTT timestamp (e.g., "00:01:23.456") to seconds
fn parse_timestamp(ts: &str) -> Option<f64> {
    let parts: Vec<&str> = ts.split(':').collect();
    if parts.len() == 3 {
        let hours: f64 = parts[0].parse().ok()?;
        let minutes: f64 = parts[1].parse().ok()?;
        let seconds: f64 = parts[2].parse().ok()?;
        Some(hours * 3600.0 + minutes * 60.0 + seconds)
    } else if parts.len() == 2 {
        let minutes: f64 = parts[0].parse().ok()?;
        let seconds: f64 = parts[1].parse().ok()?;
        Some(minutes * 60.0 + seconds)
    } else {
        None
    }
}

/// Parse a VTT file and extract subtitle segments
pub fn parse_vtt_file(file_path: &Path) -> Result<Vec<SubtitleSegment>> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut segments = Vec::new();
    
    // Regular expression for timestamps
    let re_timestamp = Regex::new(r"(\d{2}:\d{2}:\d{2}\.\d{3})\s+-->\s+(\d{2}:\d{2}:\d{2}\.\d{3})")?;

    let mut lines = reader.lines().enumerate();
    let mut index = 0;
    
    while let Some((_, Ok(line))) = lines.next() {
        if let Some(caps) = re_timestamp.captures(&line) {
            let start = parse_timestamp(&caps[1])
                .ok_or_else(|| anyhow!("Invalid start timestamp: {}", &caps[1]))?;
            let end = parse_timestamp(&caps[2])
                .ok_or_else(|| anyhow!("Invalid end timestamp: {}", &caps[2]))?;
            
            let mut text = String::new();
            // Read text until we hit an empty line or another timestamp
            while let Some((_, Ok(text_line))) = lines.next() {
                if text_line.trim().is_empty() || re_timestamp.is_match(&text_line) {
                    // If we hit another timestamp, we need to backtrack
                    if re_timestamp.is_match(&text_line) {
                        // This approach is simplistic, we'd need to handle this better in a real implementation
                        break;
                    }
                    break;
                }
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(&text_line);
            }
            
            if !text.is_empty() {
                segments.push(SubtitleSegment { start, end, text, index });
                index += 1;
            }
        }
    }
    
    Ok(segments)
}

/// Format seconds to a VTT timestamp (HH:MM:SS.mmm)
fn format_timestamp(seconds: f64) -> String {
    let hours = (seconds / 3600.0).floor() as u32;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
    let secs = seconds % 60.0;
    
    format!("{:02}:{:02}:{:06.3}", hours, minutes, secs)
}

/// Generate audio for a single subtitle segment using OpenAI TTS API
async fn generate_audio_segment(
    client: &Client,
    api_key: &str,
    segment: &SubtitleSegment,
    words_per_second: f64,
    voice: &str,
    model: &str,
    output_dir: &Path,
) -> Result<(PathBuf, f64, f64)> {  // Return tuple of (file_path, actual_duration, target_duration)
    let target_duration = segment.end - segment.start;
    let word_count = segment.text.split_whitespace().count() as f64;
    
    // Estimate duration at normal speed (speed=1.0)
    let estimated_duration = word_count / words_per_second;
    
    // Always use normal speed instead of slowing down
    let speed = if estimated_duration < target_duration {
        1.0
    } else {
        let calculated_speed = estimated_duration / target_duration;
        calculated_speed.min(4.0) // Cap at max speed of 4.0
    };
    
    // Prepare request to OpenAI TTS API
    let url = "https://api.openai.com/v1/audio/speech";
    let body = serde_json::json!({
        "model": model,
        "input": segment.text,
        "voice": voice,
        "speed": speed,
        "response_format": "mp3"
    });
    
    let resp = client
        .post(url)
        .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await?;
    
    if !resp.status().is_success() {
        let status = resp.status();
        let err_text = resp.text().await?;
        return Err(anyhow!("OpenAI API error: {} - {}", status, err_text));
    }
    
    // Save the audio data to a file
    let segment_file = output_dir.join(format!("segment_{}.mp3", segment.index));
    let bytes = resp.bytes().await?;
    fs::write(&segment_file, &bytes)?;
    
    // Get the actual duration of the generated audio using ffmpeg
    let actual_duration = get_audio_duration(&segment_file)?;
    
    Ok((segment_file, actual_duration, target_duration))
}

/// Generate TTS audio for all segments in a VTT file
pub async fn generate_tts(
    vtt_file: &Path,
    output_dir: &Path,
    api_key: &str,
    voice: &str,
    model: &str,
    words_per_second: f64,
    base_filename: &str,
    language_suffix: &str,
    progress_channel: Option<mpsc::Sender<TTSProgress>>,
) -> Result<PathBuf> {
    // Make sure output directory exists
    if !output_dir.exists() {
        fs::create_dir_all(output_dir)?;
    }

    // Define output file path with the correct format
    let language_code = language_suffix.trim_start_matches('_');
    let output_file = output_dir.join(format!("{}_{}.mp3", base_filename, language_code));
    
    // Check if TTS file already exists
    if check_file_exists_and_valid(&output_file).await {
        info!("Found existing TTS file");
        return Ok(output_file);
    }
    
    // Parse VTT file
    let segments = parse_vtt_file(vtt_file)?;
    let total_segments = segments.len();
    
    if total_segments == 0 {
        return Err(anyhow!("No segments found in the VTT file"));
    }
    
    // Report initial progress
    if let Some(sender) = &progress_channel {
        sender.send(TTSProgress {
            status: "Starting TTS generation".to_string(),
            progress: 0.0,
            current_segment: 0,
            total_segments,
        }).await?;
    }
    
    // Create HTTP client
    let client = Client::new();
    
    // Vector to store generated audio files
    let mut audio_files = Vec::new();
    
    // Generate audio for each segment
    for (i, segment) in segments.iter().enumerate() {
        if let Some(sender) = &progress_channel {
            sender.send(TTSProgress {
                status: format!("Generating speech for segment {}/{}", i + 1, total_segments),
                progress: (i as f32 / total_segments as f32) * 100.0,
                current_segment: i + 1,
                total_segments,
            }).await?;
        }
        
        let (output_file, actual_duration, target_duration) = generate_audio_segment(
            &client, 
            api_key, 
            segment, 
            words_per_second, 
            voice, 
            model, 
            output_dir
        ).await?;
        
        audio_files.push((segment.start, segment.end, output_file, actual_duration, target_duration));
    }
    
    // Create a concat file for ffmpeg
    let concat_file = output_dir.join("segments.txt");
    let mut file = File::create(&concat_file)?;
    let mut last_time = 0.0;
    
    for (start, end, file_path, actual_duration, target_duration) in &audio_files {
        // If there's a gap between segments, add silence
        if *start > last_time {
            let gap_duration = start - last_time;
            let silence_file = output_dir.join(format!("silence_gap_{:.3}.mp3", start));
            
            // Generate silence using ffmpeg
            let status = Command::new("ffmpeg")
                .args([
                    "-f", "lavfi",
                    "-i", "anullsrc=r=44100:cl=mono",
                    "-t", &gap_duration.to_string(),
                    "-q:a", "9",
                    "-acodec", "libmp3lame",
                    "-y", silence_file.to_str().unwrap(),
                ])
                .status()?;
                
            if !status.success() {
                log::warn!("Failed to create silence audio for gap of {} seconds", gap_duration);
                continue;
            }
            
            writeln!(file, "file '{}'", silence_file.to_str().unwrap())?;
            last_time = *start;
        }
        
        // Write the speech segment
        writeln!(file, "file '{}'", file_path.to_str().unwrap())?;
        
        // Add trailing silence if the actual audio is shorter than the target duration
        if *actual_duration < *target_duration {
            let silence_duration = target_duration - actual_duration;
            
            // Only add silence if it's significant enough (more than 0.1 seconds)
            if silence_duration > 0.1 {
                let segment_index = audio_files.iter().position(|s| &s.2 == file_path).unwrap_or(0);
                let silence_file = output_dir.join(format!("silence_padding_{}.mp3", segment_index));
                
                // Generate silence using ffmpeg
                let status = Command::new("ffmpeg")
                    .args([
                        "-f", "lavfi",
                        "-i", "anullsrc=r=44100:cl=mono",
                        "-t", &silence_duration.to_string(),
                        "-q:a", "9",
                        "-acodec", "libmp3lame",
                        "-y", silence_file.to_str().unwrap(),
                    ])
                    .status()?;
                    
                if !status.success() {
                    log::warn!("Failed to create padding silence of {} seconds", silence_duration);
                    continue;
                }
                
                writeln!(file, "file '{}'", silence_file.to_str().unwrap())?;
            }
        }
        
        last_time = *end;
    }
    
    // Report progress
    if let Some(sender) = &progress_channel {
        sender.send(TTSProgress {
            status: "Combining audio segments".to_string(),
            progress: 95.0,
            current_segment: total_segments,
            total_segments,
        }).await?;
    }
    
    // Combine all segments using ffmpeg
    let status = Command::new("ffmpeg")
        .args([
            "-f", "concat",
            "-safe", "0",
            "-i", concat_file.to_str().unwrap(),
            "-c", "copy",
            "-y", output_file.to_str().unwrap(),
        ])
        .status()?;
        
    if !status.success() {
        return Err(anyhow!("Failed to combine audio segments"));
    }
    
    // Clean up temporary files after successful completion
    if let Some(sender) = &progress_channel {
        sender.send(TTSProgress {
            status: "Cleaning up temporary files".to_string(),
            progress: 98.0,
            current_segment: total_segments,
            total_segments,
        }).await?;
    }

    // Collect all temporary file paths
    let mut temp_files = Vec::new();

    // Add segment audio files
    for (_, _, file_path, _, _) in &audio_files {
        temp_files.push(file_path.clone());
    }

    // Find and add silence files
    if let Ok(entries) = fs::read_dir(output_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if (filename.starts_with("silence_gap_") || filename.starts_with("silence_padding_")) && filename.ends_with(".mp3") {
                    temp_files.push(path);
                }
            }
        }
    }

    // Add concat file
    temp_files.push(concat_file.clone());

    // Delete all temporary files
    for file in temp_files {
        if let Err(e) = fs::remove_file(&file) {
            log::warn!("Failed to delete temporary file {}: {}", file.display(), e);
        }
    }

    // Final progress update
    if let Some(sender) = &progress_channel {
        sender.send(TTSProgress {
            status: "TTS generation complete".to_string(),
            progress: 100.0,
            current_segment: total_segments,
            total_segments,
        }).await?;
    }

    Ok(output_file.clone())
}

/// Get the duration of an audio file using ffmpeg
fn get_audio_duration(audio_file: &Path) -> Result<f64> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            audio_file.to_str().unwrap(),
        ])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow!("Failed to get audio duration: {}", 
            String::from_utf8_lossy(&output.stderr)));
    }
    
    let duration_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let duration = duration_str.parse::<f64>()
        .map_err(|e| anyhow!("Failed to parse audio duration: {}", e))?;
    
    Ok(duration)
} 