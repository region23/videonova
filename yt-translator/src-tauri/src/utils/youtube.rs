use anyhow::{anyhow, Result};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use tokio::sync::mpsc;
use tokio::task;

use super::tools::get_tool_path;

#[derive(Debug, Serialize, Deserialize)]
pub struct VideoInfo {
    pub title: String,
    pub duration: f64,
    pub url: String,
    pub thumbnail: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DownloadProgress {
    pub status: String,
    pub progress: f32,
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub component: String, // "audio" or "video"
}

#[derive(Debug)]
pub struct DownloadResult {
    pub video_path: PathBuf,
    pub audio_path: PathBuf,
}

/// Download video from YouTube with parallel audio and video processing
pub async fn download_video(
    url: &str,
    output_dir: &PathBuf,
    progress_sender: Option<mpsc::Sender<DownloadProgress>>,
) -> Result<DownloadResult> {
    // Get yt-dlp path
    let ytdlp_path = get_tool_path("yt-dlp")
        .ok_or_else(|| anyhow!("yt-dlp not found"))?;

    // Create output directory if it doesn't exist
    std::fs::create_dir_all(output_dir)?;

    // Get video info first to get the title
    let video_info = get_video_info(url).await?;
    let safe_title = sanitize_filename(&video_info.title);

    // Prepare output templates
    let audio_template = output_dir.join(format!("{}_audio.%(ext)s", safe_title));
    let video_template = output_dir.join(format!("{}_video.%(ext)s", safe_title));

    // Create progress channels for audio and video
    let (audio_progress_tx, mut audio_progress_rx) = mpsc::channel(32);
    let (video_progress_tx, mut video_progress_rx) = mpsc::channel(32);

    // Clone necessary values for tasks
    let url_clone = url.to_string();
    let ytdlp_path_clone = ytdlp_path.clone();
    let progress_sender_clone = progress_sender.clone();

    // Start audio download task
    let audio_task = task::spawn(async move {
        download_audio(&ytdlp_path_clone, &url_clone, &audio_template, Some(audio_progress_tx)).await
    });

    // Clone URL again for video task
    let url_clone_video = url.to_string();
    let ytdlp_path_clone_video = ytdlp_path.clone();

    // Start video download task
    let video_task = task::spawn(async move {
        download_video_only(&ytdlp_path_clone_video, &url_clone_video, &video_template, Some(video_progress_tx)).await
    });

    // Monitor progress from both downloads
    if let Some(sender) = progress_sender {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(audio_progress) = audio_progress_rx.recv() => {
                        let mut progress = audio_progress;
                        progress.component = "audio".to_string();
                        if let Err(e) = sender.send(progress).await {
                            error!("Failed to send audio progress: {}", e);
                        }
                    }
                    Some(video_progress) = video_progress_rx.recv() => {
                        let mut progress = video_progress;
                        progress.component = "video".to_string();
                        if let Err(e) = sender.send(progress).await {
                            error!("Failed to send video progress: {}", e);
                        }
                    }
                    else => break,
                }
            }
        });
    }

    // Wait for both downloads to complete
    let (audio_result, video_result) = tokio::join!(audio_task, video_task);
    
    let audio_path = audio_result??;
    let video_path = video_result??;

    Ok(DownloadResult {
        video_path,
        audio_path,
    })
}

/// Download audio only
async fn download_audio(
    ytdlp_path: &PathBuf,
    url: &str,
    output_template: &PathBuf,
    progress_sender: Option<mpsc::Sender<DownloadProgress>>,
) -> Result<PathBuf> {
    let mut command = Command::new(ytdlp_path);
    command
        .arg(url)
        .arg("--format").arg("bestaudio[ext=m4a]/bestaudio")
        .arg("--extract-audio")
        .arg("--audio-format").arg("m4a")
        .arg("--output").arg(output_template.as_os_str())
        .arg("--newline")
        .arg("--progress")
        .arg("--no-playlist")
        .arg("--no-warnings")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    process_download(command, progress_sender).await
}

/// Download video only (no audio)
async fn download_video_only(
    ytdlp_path: &PathBuf,
    url: &str,
    output_template: &PathBuf,
    progress_sender: Option<mpsc::Sender<DownloadProgress>>,
) -> Result<PathBuf> {
    let mut command = Command::new(ytdlp_path);
    command
        .arg(url)
        .arg("--format").arg("bestvideo[ext=mp4]/bestvideo")
        .arg("--output").arg(output_template.as_os_str())
        .arg("--newline")
        .arg("--progress")
        .arg("--no-playlist")
        .arg("--no-warnings")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    process_download(command, progress_sender).await
}

/// Process download command and handle progress
async fn process_download(
    mut command: Command,
    progress_sender: Option<mpsc::Sender<DownloadProgress>>,
) -> Result<PathBuf> {
    let mut child = command.spawn()?;
    
    let stdout = child.stdout.take()
        .ok_or_else(|| anyhow!("Failed to get stdout handle"))?;

    let reader = std::io::BufReader::new(stdout);
    let lines = std::io::BufRead::lines(reader);

    for line in lines {
        if let Ok(line) = line {
            debug!("yt-dlp output: {}", line);
            
            if let Some(progress) = parse_progress(&line) {
                if let Some(sender) = &progress_sender {
                    if let Err(e) = sender.send(progress).await {
                        error!("Failed to send progress: {}", e);
                    }
                }
            }
        }
    }

    let status = child.wait()?;
    if !status.success() {
        return Err(anyhow!("yt-dlp failed with status: {}", status));
    }

    // Find the output file (most recent file in directory)
    let parent = PathBuf::from(command.get_current_dir().unwrap_or(&std::env::current_dir()?));
    let entries = std::fs::read_dir(parent)?;
    let mut newest_file = None;
    let mut newest_time = std::time::UNIX_EPOCH;

    for entry in entries {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if let Ok(modified) = metadata.modified() {
            if modified > newest_time {
                newest_time = modified;
                newest_file = Some(entry.path());
            }
        }
    }

    newest_file.ok_or_else(|| anyhow!("Failed to find downloaded file"))
}

/// Get video information without downloading
pub async fn get_video_info(url: &str) -> Result<VideoInfo> {
    // Get yt-dlp path
    let ytdlp_path = get_tool_path("yt-dlp")
        .ok_or_else(|| anyhow!("yt-dlp not found"))?;

    // Prepare command to get video info in JSON format
    let output = Command::new(ytdlp_path)
        .arg(url)
        .arg("--dump-json")
        .arg("--no-playlist")
        .output()?;

    if !output.status.success() {
        return Err(anyhow!("Failed to get video info"));
    }

    // Parse JSON output
    let json = String::from_utf8_lossy(&output.stdout);
    let info: serde_json::Value = serde_json::from_str(&json)?;

    Ok(VideoInfo {
        title: info["title"].as_str().unwrap_or("Unknown").to_string(),
        duration: info["duration"].as_f64().unwrap_or(0.0),
        url: url.to_string(),
        thumbnail: info["thumbnail"].as_str().unwrap_or("").to_string(),
        description: info["description"].as_str().unwrap_or("").to_string(),
    })
}

/// Parse progress information from yt-dlp output
fn parse_progress(line: &str) -> Option<DownloadProgress> {
    if !line.starts_with("[download]") {
        return None;
    }

    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }

    let progress = parts[1]
        .trim_end_matches('%')
        .parse::<f32>()
        .ok()?;

    let speed = if let Some(speed_idx) = parts.iter().position(|&p| p == "at") {
        if parts.len() > speed_idx + 1 {
            Some(parts[speed_idx + 1].to_string())
        } else {
            None
        }
    } else {
        None
    };

    let eta = if let Some(eta_idx) = parts.iter().position(|&p| p == "ETA") {
        if parts.len() > eta_idx + 1 {
            Some(parts[eta_idx + 1].to_string())
        } else {
            None
        }
    } else {
        None
    };

    Some(DownloadProgress {
        status: "downloading".to_string(),
        progress,
        speed,
        eta,
        component: "unknown".to_string(), // Will be set by the caller
    })
}

/// Sanitize filename to be safe for all operating systems
fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c
        })
        .collect()
} 