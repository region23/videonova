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

// Add a new structure to control the ffmpeg process
struct FfmpegMonitor {
    pid: u32,
    is_stuck: bool,
    last_activity: Instant,
}

// Monitor ffmpeg process for hangs and timeouts
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

    let mut cmd = TokioCommand::new("ffmpeg");
    cmd.arg("-y")
        .arg("-i")
        .arg(original_vtt_path)
        .arg(&original_ass);

    let output = cmd.output().await?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to convert original subtitles: {}", error).into());
    }

    // Convert translated VTT to ASS
    let mut cmd = TokioCommand::new("ffmpeg");
    cmd.arg("-y")
        .arg("-i")
        .arg(translated_vtt_path)
        .arg(&translated_ass);

    let output = cmd.output().await?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to convert translated subtitles: {}", error).into());
    }

    if let Some(tx) = &progress_tx {
        tx.send(MergeProgress {
            status: "Merging video and audio".to_string(),
            progress: 20.0,
        })
        .await?;
    }

    // Prepare final merge command
    let mut cmd = TokioCommand::new("ffmpeg");
    cmd.arg("-y") // Overwrite output file if it exists
        .arg("-i")
        .arg(video_path)
        .arg("-i")
        .arg(translated_audio_path)
        .arg("-i")
        .arg(original_audio_path)
        .arg("-i")
        .arg(&original_ass)
        .arg("-i")
        .arg(&translated_ass)
        .arg("-map")
        .arg("0:v") // Video stream
        .arg("-map")
        .arg("1:a") // First audio track: Translated + Instrumental (final_mixed.wav)
        .arg("-map")
        .arg("2:a") // Second audio track: Original
        .arg("-map")
        .arg("3") // Original subtitles
        .arg("-map")
        .arg("4") // Translated subtitles
        // Video settings for compatibility
        .arg("-c:v")
        .arg("libx264")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-profile:v")
        .arg("high")
        .arg("-level")
        .arg("4.1")
        // Audio settings
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("192k")
        // Subtitle settings
        .arg("-c:s")
        .arg("mov_text") // Using standard mov_text encoder
        .arg("-disposition:s:0")
        .arg("none")
        .arg("-disposition:s:1")
        .arg("none")
        // QuickTime specific compatibility flags
        .arg("-movflags")
        .arg("+faststart+rtphint")
        .arg("-tag:v")
        .arg("avc1")
        .arg("-tag:a")
        .arg("mp4a")
        // Set metadata for audio tracks
        // First audio track (translated + instrumental)
        .arg("-metadata:s:a:0")
        .arg(format!("language={}", convert_to_iso_639_2(target_language_code)))
        .arg("-metadata:s:a:0")
        .arg(format!("title={} Audio", target_language_name))
        .arg("-metadata:s:a:0")
        .arg("handler_name=Audio Track (Translated)")
        .arg("-disposition:a:0")
        .arg("default")
        // Second audio track (original)
        .arg("-metadata:s:a:1")
        .arg(format!("language={}", convert_to_iso_639_2(source_language_code)))
        .arg("-metadata:s:a:1")
        .arg(format!("title={} Audio", source_language_name))
        .arg("-metadata:s:a:1")
        .arg("handler_name=Audio Track (Original)")
        .arg("-disposition:a:1")
        .arg("none")
        // Subtitle metadata
        .arg("-metadata:s:s:0")
        .arg(format!("language={}", convert_to_iso_639_2(source_language_code)))
        .arg("-metadata:s:s:0")
        .arg(format!("title={} Subtitles", source_language_name))
        .arg("-metadata:s:s:0")
        .arg("handler_name=Subtitles (Original)")
        .arg("-metadata:s:s:1")
        .arg(format!("language={}", convert_to_iso_639_2(target_language_code)))
        .arg("-metadata:s:s:1")
        .arg(format!("title={} Subtitles", target_language_name))
        .arg("-metadata:s:s:1")
        .arg("handler_name=Subtitles (Translated)")
        .arg(&output_path);

    log::info!("Executing ffmpeg command: {:?}", cmd);

    // Execute ffmpeg with progress monitoring
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;

    // Monitor progress
    let pid = child.id().ok_or("Failed to get process ID")?;
    let monitor = Arc::new(Mutex::new(FfmpegMonitor {
        pid,
        is_stuck: false,
        last_activity: Instant::now(),
    }));

    // Spawn monitoring task
    let monitor_clone = monitor.clone();
    tokio::spawn(async move {
        monitor_ffmpeg_process(pid, monitor_clone).await;
    });

    // Wait for completion with timeout
    let status = match timeout(Duration::from_secs(600), child.wait()).await {
        Ok(result) => result?,
        Err(_) => {
            error!("ffmpeg process timed out after 10 minutes");
            return Err("ffmpeg process timed out after 10 minutes".into());
        }
    };

    if !status.success() {
        let mut stderr_content = Vec::new();
        if let Some(mut stderr) = child.stderr {
            if let Err(e) = stderr.read_to_end(&mut stderr_content).await {
                error!("Failed to read stderr: {}", e);
                return Err("Failed to read ffmpeg error output".into());
            }
        }
        let error_message = String::from_utf8_lossy(&stderr_content);
        error!("ffmpeg error: {}", error_message);
        return Err(format!("ffmpeg failed: {}", error_message).into());
    }

    // Clean up temporary subtitle files
    let _ = tokio::fs::remove_file(&original_ass).await;
    let _ = tokio::fs::remove_file(&translated_ass).await;

    // Send completion progress
    if let Some(tx) = &progress_tx {
        tx.send(MergeProgress {
            status: "Merge complete".to_string(),
            progress: 100.0,
        })
        .await?;
    }

    Ok(output_path.to_path_buf())
}

/// Convert ISO 639-1 two-letter language code to ISO 639-2 three-letter code
fn convert_to_iso_639_2(code: &str) -> String {
    match code {
        "en" => "eng",
        "ru" => "rus",
        "es" => "spa",
        "fr" => "fra",
        "de" => "deu",
        "it" => "ita",
        "pt" => "por",
        "ja" => "jpn",
        "ko" => "kor",
        "zh" => "zho",
        // Add more mappings as needed
        _ => code, // Return original code if no mapping found
    }.to_string()
}
