use anyhow::Result;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command as TokioCommand;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};

/// Structure for holding merge progress information
#[derive(Clone, Serialize, Deserialize)]
pub struct MergeProgress {
    pub status: String,
    pub progress: f32,
}

/// Internal structure to control the ffmpeg process
struct FfmpegMonitor {
    pid: u32,
    is_stuck: bool,
    last_activity: Instant,
}

/// Monitor ffmpeg process for hangs and timeouts
///
/// # Arguments
/// * `pid` - Process ID of the ffmpeg process to monitor
/// * `monitor` - Shared monitor structure to track process state
async fn monitor_ffmpeg_process(pid: u32, monitor: Arc<Mutex<FfmpegMonitor>>) {
    let start_time = Instant::now();
    let monitor_interval = Duration::from_secs(5); // Check every 5 seconds

    loop {
        sleep(monitor_interval).await;

        // Check if process is still running
        #[cfg(target_family = "unix")]
        let process_exists = std::process::Command::new("ps")
            .args(["-p", &pid.to_string()])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        #[cfg(target_family = "windows")]
        let process_exists = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
            .map(|output| {
                let output_str = String::from_utf8_lossy(&output.stdout);
                output_str.contains(&pid.to_string())
            })
            .unwrap_or(false);

        if !process_exists {
            info!(
                "ffmpeg process {} no longer exists, monitoring stopped",
                pid
            );
            break;
        }

        let mut is_stuck = false;

        // Get process stats
        #[cfg(target_family = "unix")]
        {
            if let Ok(output) = std::process::Command::new("ps")
                .args(["-o", "%cpu,%mem", "-p", &pid.to_string()])
                .output()
            {
                let output_str = String::from_utf8_lossy(&output.stdout);
                info!(
                    "ffmpeg (PID: {}) running for {:?}: {}",
                    pid,
                    start_time.elapsed(),
                    output_str.trim()
                );

                // Parse the CPU usage
                let lines: Vec<&str> = output_str.lines().collect();
                if lines.len() >= 2 {
                    let stats = lines[1].trim();
                    if let Some(cpu_str) = stats.split_whitespace().next() {
                        if let Ok(cpu) = cpu_str.trim().parse::<f32>() {
                            if cpu < 0.5 {
                                warn!(
                                    "ffmpeg process has very low CPU usage ({}%), possibly stuck",
                                    cpu
                                );
                                is_stuck = true;
                            }
                        }
                    }
                }
            }
        }

        #[cfg(target_family = "windows")]
        {
            if let Ok(output) = std::process::Command::new("wmic")
                .args([
                    "process",
                    "where",
                    &format!("ProcessId={}", pid),
                    "get",
                    "PercentProcessorTime",
                ])
                .output()
            {
                let output_str = String::from_utf8_lossy(&output.stdout);
                info!(
                    "ffmpeg (PID: {}) running for {:?}: {}",
                    pid,
                    start_time.elapsed(),
                    output_str.trim()
                );

                // Parse CPU usage for Windows
                let lines: Vec<&str> = output_str.lines().collect();
                if lines.len() >= 2 {
                    let cpu_str = lines[1].trim();
                    if let Ok(cpu) = cpu_str.parse::<u32>() {
                        if cpu < 1 {
                            warn!(
                                "ffmpeg process has very low CPU usage ({}%), possibly stuck",
                                cpu
                            );
                            is_stuck = true;
                        }
                    }
                }
            }
        }

        // Update the monitor
        let mut monitor_guard = monitor.lock().await;

        // Check if there's been no activity for a while
        let elapsed = monitor_guard.last_activity.elapsed();
        if elapsed > Duration::from_secs(60) {
            warn!(
                "No activity from ffmpeg for {} seconds, marking as stuck",
                elapsed.as_secs()
            );
            is_stuck = true;
        }

        if is_stuck {
            monitor_guard.is_stuck = true;

            // If process is stuck for more than 2 minutes, kill it
            if start_time.elapsed() > Duration::from_secs(120) && monitor_guard.is_stuck {
                error!("ffmpeg process appears to be stuck for more than 2 minutes, killing it");

                #[cfg(target_family = "unix")]
                {
                    if let Err(e) = std::process::Command::new("kill")
                        .args(["-9", &pid.to_string()])
                        .output()
                    {
                        error!("Failed to kill stuck ffmpeg process: {}", e);
                    } else {
                        info!("Successfully killed stuck ffmpeg process {}", pid);
                    }
                }

                #[cfg(target_family = "windows")]
                {
                    if let Err(e) = std::process::Command::new("taskkill")
                        .args(["/F", "/PID", &pid.to_string()])
                        .output()
                    {
                        error!("Failed to kill stuck ffmpeg process: {}", e);
                    } else {
                        info!("Successfully killed stuck ffmpeg process {}", pid);
                    }
                }

                break;
            }
        } else {
            // Reset the stuck flag if process is active
            monitor_guard.is_stuck = false;
            monitor_guard.last_activity = Instant::now();
        }
    }
}

/// Merge video, audio, and subtitles files using ffmpeg
///
/// # Arguments
/// * `video_path` - Path to the original video file
/// * `translated_audio_path` - Path to the translated audio file
/// * `original_audio_path` - Path to the original audio file
/// * `original_vtt_path` - Path to the original subtitles file in VTT format
/// * `translated_vtt_path` - Path to the translated subtitles file in VTT format
/// * `output_path` - Path where the merged video will be saved
/// * `source_language_code` - ISO code of the source language (e.g., "en")
/// * `target_language_code` - ISO code of the target language (e.g., "es")
/// * `source_language_name` - Full name of the source language (e.g., "English")
/// * `target_language_name` - Full name of the target language (e.g., "Spanish")
/// * `progress_tx` - Optional channel for sending progress updates
///
/// # Returns
/// * `Result<PathBuf>` - Path to the merged video file on success, or an error
pub async fn merge_files(
    video_path: &Path,
    translated_audio_path: &Path,
    original_audio_path: &Path,
    original_vtt_path: &Path,
    translated_vtt_path: &Path,
    output_path: &Path,
    source_language_code: &str,
    target_language_code: &str,
    source_language_name: &str,
    target_language_name: &str,
    progress_tx: Option<mpsc::Sender<MergeProgress>>,
) -> Result<PathBuf, Box<dyn StdError + Send + Sync>> {
    log::info!("=== MERGE_FILES FUNCTION CALLED ===");
    log::info!("Input parameters:");
    log::info!("  Video: {}", video_path.display());
    log::info!("  Translated Audio: {}", translated_audio_path.display());
    log::info!("  Original Audio: {}", original_audio_path.display());
    log::info!("  Original VTT: {}", original_vtt_path.display());
    log::info!("  Translated VTT: {}", translated_vtt_path.display());
    log::info!("  Output Path: {}", output_path.display());

    // Get the output directory from the output path
    let output_dir = output_path.parent()
        .ok_or("Invalid output path: no parent directory")?;

    // Create output directory if it doesn't exist
    tokio::fs::create_dir_all(output_dir)
        .await
        .map_err(|e| format!("Failed to create output directory: {}", e))?;

    // Get video filename without extension for temporary files
    let video_stem = video_path
        .file_stem()
        .ok_or("Invalid video filename")?
        .to_str()
        .ok_or("Invalid video filename encoding")?;

    // Send initial progress
    if let Some(tx) = &progress_tx {
        tx.send(MergeProgress {
            status: "Starting merge process".to_string(),
            progress: 0.0,
        })
        .await?;
    }

    // Convert VTT to ASS format for ffmpeg
    let original_ass = output_dir.join(format!("{}_original.ass", video_stem));
    let translated_ass = output_dir.join(format!("{}_translated.ass", video_stem));

    // Convert original VTT to ASS
    if let Some(tx) = &progress_tx {
        tx.send(MergeProgress {
            status: "Converting subtitles".to_string(),
            progress: 10.0,
        })
        .await?;
    }

    // Build ffmpeg command
    let mut original_sub_args = vec![
        "-i".to_string(),
        original_vtt_path.to_string_lossy().to_string(),
        // Font properties
        "-c:s".to_string(),
        "ass".to_string(),
        // Set ASS style
        "-metadata:s:s:0".to_string(),
        format!("title={}", source_language_name),
        "-metadata:s:s:0".to_string(),
        format!("language={}", convert_to_iso_639_2(source_language_code)),
    ];

    // Add translation subtitles if provided
    let mut translated_sub_args = vec![];
    if translated_vtt_path.exists() {
        translated_sub_args = vec![
            "-i".to_string(),
            translated_vtt_path.to_string_lossy().to_string(),
            // Font properties
            "-c:s".to_string(),
            "ass".to_string(),
            // Set ASS style
            "-metadata:s:s:1".to_string(),
            format!("title={}", target_language_name),
            "-metadata:s:s:1".to_string(),
            format!("language={}", convert_to_iso_639_2(target_language_code)),
        ];
    }

    // Create FFmpeg command to merge all streams
    if let Some(tx) = &progress_tx {
        tx.send(MergeProgress {
            status: "Preparing to merge streams".to_string(),
            progress: 20.0,
        })
        .await?;
    }

    // Prepare ffmpeg arguments
    let mut ffmpeg_args = vec![
        "-i".to_string(),
        video_path.to_string_lossy().to_string(),
        "-i".to_string(),
        translated_audio_path.to_string_lossy().to_string(),
        "-i".to_string(),
        original_audio_path.to_string_lossy().to_string(),
    ];

    // Add subtitle input arguments
    ffmpeg_args.extend(original_sub_args);
    if !translated_sub_args.is_empty() {
        ffmpeg_args.extend(translated_sub_args);
    }

    // Mapping and encoding arguments
    ffmpeg_args.extend(vec![
        // Map video stream
        "-map".to_string(),
        "0:v:0".to_string(),
        // Map translated audio as first audio track
        "-map".to_string(),
        "1:a:0".to_string(),
        // Map original audio as second audio track
        "-map".to_string(),
        "2:a:0".to_string(),
        // Map original subtitles
        "-map".to_string(),
        "3:s:0".to_string(),
    ]);

    // Map translated subtitles if they exist
    if translated_vtt_path.exists() {
        ffmpeg_args.push("-map".to_string());
        ffmpeg_args.push("4:s:0".to_string());
    }

    // Set metadata for audio tracks
    ffmpeg_args.extend(vec![
        // Set metadata for translated audio
        "-metadata:s:a:0".to_string(),
        format!("title={}", target_language_name),
        "-metadata:s:a:0".to_string(),
        format!("language={}", convert_to_iso_639_2(target_language_code)),
        // Set metadata for original audio
        "-metadata:s:a:1".to_string(),
        format!("title={}", source_language_name),
        "-metadata:s:a:1".to_string(),
        format!("language={}", convert_to_iso_639_2(source_language_code)),
    ]);

    // Final encoding settings
    ffmpeg_args.extend(vec![
        "-c:v".to_string(),
        "copy".to_string(), // Copy video codec
        "-c:a".to_string(),
        "aac".to_string(), // Use AAC for audio
        "-b:a".to_string(),
        "192k".to_string(), // Audio bitrate
        "-c:s".to_string(),
        "mov_text".to_string(), // Use mov_text for subtitles in MP4
        "-movflags".to_string(),
        "+faststart".to_string(), // Optimize for streaming
        "-y".to_string(), // Overwrite output file if it exists
        output_path.to_string_lossy().to_string(),
    ]);

    // Log full ffmpeg command
    info!(
        "Running FFmpeg command: ffmpeg {}",
        ffmpeg_args.join(" ")
    );

    // Create the FFmpeg command
    let mut cmd = TokioCommand::new("ffmpeg");
    cmd.args(&ffmpeg_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Launch the process
    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to start ffmpeg: {}", e))?;

    // Get the process ID for monitoring
    let pid = child.id().ok_or("Failed to get ffmpeg process ID")?;
    
    // Create a monitor for the ffmpeg process
    let monitor = Arc::new(Mutex::new(FfmpegMonitor {
        pid,
        is_stuck: false,
        last_activity: Instant::now(),
    }));
    
    // Spawn a task to monitor the process
    let monitor_clone = monitor.clone();
    tokio::spawn(async move {
        monitor_ffmpeg_process(pid, monitor_clone).await;
    });

    // Monitor the progress by reading the ffmpeg output
    if let Some(stderr) = child.stderr.take() {
        let monitor_clone = monitor.clone();
        let progress_tx_clone = progress_tx.clone();
        
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(stderr).lines();
            
            while let Ok(Some(line)) = lines.next_line().await {
                // Update last activity time
                let mut monitor = monitor_clone.lock().await;
                monitor.last_activity = Instant::now();
                drop(monitor);
                
                // Parse progress information
                if line.contains("time=") {
                    if let Some(tx) = &progress_tx_clone {
                        // Extract time information
                        if let Some(time_str) = line.split("time=").nth(1) {
                            if let Some(time) = time_str.split(' ').next() {
                                // Parse the time (format: HH:MM:SS.MS)
                                let parts: Vec<&str> = time.split(':').collect();
                                if parts.len() >= 3 {
                                    let hours: f32 = parts[0].parse().unwrap_or(0.0);
                                    let minutes: f32 = parts[1].parse().unwrap_or(0.0);
                                    let seconds: f32 = parts[2].parse().unwrap_or(0.0);
                                    
                                    let total_seconds = hours * 3600.0 + minutes * 60.0 + seconds;
                                    // Assuming a 3-minute video (180 seconds) for progress calculation
                                    // Adjust this formula based on your average video length
                                    let progress = 20.0 + (total_seconds / 180.0) * 70.0;
                                    let progress = progress.min(90.0); // Cap at 90%
                                    
                                    // Send progress update
                                    let _ = tx.send(MergeProgress {
                                        status: format!("Merging streams: {}", time),
                                        progress,
                                    }).await;
                                }
                            }
                        }
                    }
                }
                // Log output
                info!("ffmpeg: {}", line);
            }
        });
    }

    // Wait for the process to complete with a timeout
    match timeout(Duration::from_secs(1800), child.wait()).await {
        Ok(status_result) => {
            match status_result {
                Ok(status) => {
                    if status.success() {
                        info!("FFmpeg process completed successfully");
                        
                        // Send completion progress
                        if let Some(tx) = &progress_tx {
                            tx.send(MergeProgress {
                                status: "Merge completed".to_string(),
                                progress: 100.0,
                            })
                            .await?;
                        }
                        
                        Ok(output_path.to_path_buf())
                    } else {
                        error!("FFmpeg process failed with status: {}", status);
                        Err(format!("FFmpeg process failed with status: {}", status).into())
                    }
                }
                Err(e) => {
                    error!("Error waiting for FFmpeg process: {}", e);
                    Err(format!("Error waiting for FFmpeg process: {}", e).into())
                }
            }
        }
        Err(_) => {
            error!("FFmpeg process timed out after 30 minutes");
            
            // Kill the process
            let _ = child.kill().await;
            
            Err("FFmpeg process timed out after 30 minutes".into())
        }
    }
}

/// Convert ISO 639-1 language code to ISO 639-2 (needed for MP4 containers)
fn convert_to_iso_639_2(code: &str) -> String {
    match code {
        "en" => "eng",
        "es" => "spa",
        "fr" => "fra",
        "de" => "deu",
        "it" => "ita",
        "pt" => "por",
        "ru" => "rus",
        "ja" => "jpn",
        "zh" => "zho",
        "ko" => "kor",
        "ar" => "ara",
        "hi" => "hin",
        "tr" => "tur",
        "pl" => "pol",
        "nl" => "nld",
        "sv" => "swe",
        "fi" => "fin",
        "da" => "dan",
        "no" => "nor",
        "hu" => "hun",
        "cs" => "ces",
        "el" => "ell",
        "he" => "heb",
        "th" => "tha",
        "vi" => "vie",
        "uk" => "ukr",
        // For any other code, return the original or append 'a'
        _ => {
            if code.len() == 2 {
                format!("{}a", code)
            } else {
                code.to_string()
            }
        }
    }.to_string()
} 