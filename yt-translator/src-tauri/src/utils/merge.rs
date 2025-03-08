use log::{debug, info, error, warn};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use anyhow::{anyhow, Result, Error};
use serde::{Deserialize, Serialize};
use tokio::process::{Command as TokioCommand};
use tokio::sync::mpsc;
use std::time::{Instant, Duration};
use tokio::time::{timeout, sleep};
use which;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::utils::common::check_file_exists_and_valid;

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
            info!("ffmpeg process {} no longer exists, monitoring stopped", pid);
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
                info!("ffmpeg (PID: {}) running for {:?}: {}", 
                      pid, start_time.elapsed(), output_str.trim());
                
                // Parse the CPU usage
                let lines: Vec<&str> = output_str.lines().collect();
                if lines.len() >= 2 {
                    let stats = lines[1].trim();
                    if let Some(cpu_str) = stats.split_whitespace().next() {
                        if let Ok(cpu) = cpu_str.trim().parse::<f32>() {
                            if cpu < 0.5 {
                                warn!("ffmpeg process has very low CPU usage ({}%), possibly stuck", cpu);
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
                .args(["process", "where", &format!("ProcessId={}", pid), "get", "PercentProcessorTime"])
                .output()
            {
                let output_str = String::from_utf8_lossy(&output.stdout);
                info!("ffmpeg (PID: {}) running for {:?}: {}", 
                      pid, start_time.elapsed(), output_str.trim());
                
                // Parse CPU usage for Windows
                let lines: Vec<&str> = output_str.lines().collect();
                if lines.len() >= 2 {
                    let cpu_str = lines[1].trim();
                    if let Ok(cpu) = cpu_str.parse::<u32>() {
                        if cpu < 1 {
                            warn!("ffmpeg process has very low CPU usage ({}%), possibly stuck", cpu);
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
            warn!("No activity from ffmpeg for {} seconds, marking as stuck", elapsed.as_secs());
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
    output_dir: &Path,
    progress_sender: Option<mpsc::Sender<MergeProgress>>,
) -> Result<PathBuf> {
    // Track overall execution time
    let start_time = Instant::now();
    info!("===== MERGE PROCESS STARTED =====");
    info!("Input parameters:");
    info!("  Video: {}", video_path.display());
    info!("  Translated Audio: {}", translated_audio_path.display());
    info!("  Original Audio: {}", original_audio_path.display());
    info!("  Original VTT: {}", original_vtt_path.display());
    info!("  Translated VTT: {}", translated_vtt_path.display());
    info!("  Output Dir: {}", output_dir.display());
    
    // Extract base filename and language code from translated VTT path
    let translated_vtt_stem = translated_vtt_path
        .file_stem()
        .ok_or_else(|| anyhow!("Failed to get file stem from translated VTT path"))?
        .to_string_lossy();

    // The translated VTT filename should be in format "{base_name}_{lang_code}.vtt"
    let parts: Vec<&str> = translated_vtt_stem.split('_').collect();
    let lang_code = parts.last()
        .ok_or_else(|| anyhow!("Invalid translated VTT filename format"))?;

    // Extract base filename from video path
    let filename = video_path
        .file_stem()
        .ok_or_else(|| anyhow!("Failed to get file stem from video path"))?
        .to_string_lossy()
        .to_string();

    // Create output file path with language code
    let output_file = output_dir.join(format!("{}_{}.mkv", filename, lang_code));
    info!("Output file will be: {}", output_file.display());

    // Check if merged file already exists
    if check_file_exists_and_valid(&output_file).await {
        info!("Found existing merged file, skipping merge process");
        return Ok(output_file);
    }

    // Если финальный файл не существует, проверяем наличие всех входных файлов
    info!("Final merged file does not exist, checking input files...");
    
    // Добавляем проверку, что видео и аудио пути не являются директориями
    // Для каждого пути проверяем, что:
    // 1. Это файл, а не директория
    // 2. Имеет соответствующее расширение
    
    // Проверяем видеофайл - должен быть файлом с расширением медиа
    if video_path.is_dir() {
        error!("Video path points to a directory, not a file: {}", video_path.display());
        return Err(anyhow!("Video path points to a directory: {}", video_path.display()));
    }
    
    let video_ext = video_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !["mp4", "mkv", "webm", "avi", "mov"].contains(&video_ext.to_lowercase().as_str()) {
        warn!("Video file has unusual extension: {}", video_ext);
    }
    
    // Проверяем аудиофайл
    if translated_audio_path.is_dir() {
        error!("Audio path points to a directory, not a file: {}", translated_audio_path.display());
        return Err(anyhow!("Audio path points to a directory: {}", translated_audio_path.display()));
    }
    
    let audio_ext = translated_audio_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !["mp3", "m4a", "aac", "wav", "ogg"].contains(&audio_ext.to_lowercase().as_str()) {
        warn!("Audio file has unusual extension: {}", audio_ext);
    }
    
    // Проверяем файлы субтитров
    if original_vtt_path.is_dir() {
        error!("Original VTT path points to a directory, not a file: {}", original_vtt_path.display());
        return Err(anyhow!("Original VTT path points to a directory: {}", original_vtt_path.display()));
    }
    
    if translated_vtt_path.is_dir() {
        error!("Translated VTT path points to a directory, not a file: {}", translated_vtt_path.display());
        return Err(anyhow!("Translated VTT path points to a directory: {}", translated_vtt_path.display()));
    }
    
    // Проверяем пути к VTT файлам - должны иметь расширение .vtt
    if !original_vtt_path.extension().and_then(|e| e.to_str()).unwrap_or("").eq_ignore_ascii_case("vtt") {
        warn!("Original subtitles file doesn't have .vtt extension: {}", original_vtt_path.display());
    }
    
    if !translated_vtt_path.extension().and_then(|e| e.to_str()).unwrap_or("").eq_ignore_ascii_case("vtt") {
        warn!("Translated subtitles file doesn't have .vtt extension: {}", translated_vtt_path.display());
    }
    
    // Выводим дополнительную отладочную информацию для исследования корректности путей
    info!("Additional path checks:");
    info!("  Video file exists: {}", video_path.exists());
    info!("  Audio file exists: {}", translated_audio_path.exists());
    info!("  Original VTT exists: {}", original_vtt_path.exists());
    info!("  Translated VTT exists: {}", translated_vtt_path.exists());
    
    // Check file existence with detailed errors
    if !video_path.exists() {
        error!("Video file missing: {}", video_path.display());
        return Err(anyhow!("Video file not found at {}", video_path.display()));
    }
    
    if !translated_audio_path.exists() {
        error!("Translated audio file missing: {}", translated_audio_path.display());
        return Err(anyhow!("Translated audio file not found at {}", translated_audio_path.display()));
    }
    
    if !original_vtt_path.exists() {
        error!("Original subtitles file missing: {}", original_vtt_path.display());
        return Err(anyhow!("Original subtitles file not found at {}", original_vtt_path.display()));
    }
    
    if !translated_vtt_path.exists() {
        error!("Translated subtitles file missing: {}", translated_vtt_path.display());
        return Err(anyhow!("Translated subtitles file not found at {}", translated_vtt_path.display()));
    }
    
    // Create output directory if it doesn't exist
    if !output_dir.exists() {
        info!("Creating output directory: {}", output_dir.display());
        match tokio::fs::create_dir_all(output_dir).await {
            Ok(_) => info!("Output directory created successfully"),
            Err(e) => {
                error!("Failed to create output directory: {}", e);
                return Err(anyhow!("Failed to create output directory: {}", e));
            }
        }
    }
    
    // Report initial progress
    if let Some(sender) = &progress_sender {
        info!("Sending initial progress update");
        if let Err(e) = sender
            .send(MergeProgress {
                status: "Starting merge process".to_string(),
                progress: 0.0,
            })
            .await
        {
            warn!("Failed to send progress: {}", e);
        }
    }
    
    // Get original audio track from video file
    // We'll extract metadata to understand what streams we have
    info!("Checking video file details before ffprobe analysis");
    let video_metadata = match tokio::fs::metadata(video_path).await {
        Ok(metadata) => {
            info!("Video file size: {} bytes", metadata.len());
            if metadata.len() == 0 {
                error!("Video file is empty (zero bytes)");
                return Err(anyhow!("Video file is empty (zero bytes)"));
            }
            metadata
        },
        Err(e) => {
            error!("Failed to get video file metadata: {}", e);
            return Err(anyhow!("Failed to get video file metadata: {}", e));
        }
    };

    // Используем явные аргументы, чтобы получить больше информации в случае ошибки
    let ffprobe_args = [
        "-v", "error",     // Изменили с "quiet" на "error", чтобы видеть ошибки
        "-print_format", "json",
        "-show_streams",
        video_path.to_str().unwrap_or("<invalid path>"),
    ];

    // Составляем полную командную строку для логирования
    let full_cmd = format!("ffprobe {}", ffprobe_args.join(" "));
    info!("Running ffprobe command: {}", full_cmd);

    // Используем Command с проверкой наличия ffprobe
    let ffprobe_path = match which::which("ffprobe") {
        Ok(path) => {
            info!("Using ffprobe from path: {}", path.display());
            path
        },
        Err(e) => {
            error!("Failed to locate ffprobe executable: {}", e);
            return Err(anyhow!("ffprobe not found in PATH: {}", e));
        }
    };

    let ffprobe_start = Instant::now();

    // Используем новый подход к запуску команды
    let ffprobe_cmd = Command::new(&ffprobe_path)
        .args(&ffprobe_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let ffprobe_cmd = match ffprobe_cmd {
        Ok(cmd) => cmd,
        Err(e) => {
            error!("Failed to spawn ffprobe command: {}", e);
            return Err(anyhow!("Failed to spawn ffprobe: {}", e));
        }
    };

    // Добавляем более длительный таймаут для ffprobe
    let ffprobe_result = match timeout(Duration::from_secs(60), async {
        match ffprobe_cmd.wait_with_output() {
            Ok(output) => Ok(output),
            Err(e) => Err(anyhow!("ffprobe process failed: {}", e))
        }
    }).await {
        Ok(result) => result,
        Err(_) => {
            error!("ffprobe timed out after 60 seconds");
            return Err(anyhow!("ffprobe timed out after 60 seconds"));
        }
    }?;

    info!("ffprobe completed in {:?}", ffprobe_start.elapsed());

    // Проверяем результат выполнения с подробным логированием
    if !ffprobe_result.status.success() {
        let stdout = String::from_utf8_lossy(&ffprobe_result.stdout);
        let stderr = String::from_utf8_lossy(&ffprobe_result.stderr);
        error!("ffprobe failed with status: {}", ffprobe_result.status);
        error!("ffprobe stdout: {}", stdout);
        error!("ffprobe stderr: {}", stderr);
        error!("Video file path: {}", video_path.display());
        
        // Проверяем, существует ли файл в момент ошибки
        if !video_path.exists() {
            error!("Video file does not exist at the time of ffprobe execution!");
            return Err(anyhow!("Video file not found at execution time: {}", video_path.display()));
        }
        
        return Err(anyhow!("Failed to get video metadata. Status: {}. Error: {}", 
                         ffprobe_result.status, stderr));
    }
    
    // Find indices for original audio and video tracks
    info!("Parsing ffprobe JSON output...");
    let json: serde_json::Value = match serde_json::from_slice(&ffprobe_result.stdout) {
        Ok(json) => json,
        Err(e) => {
            error!("Failed to parse ffprobe JSON: {}", e);
            return Err(anyhow!("Failed to parse ffprobe output: {}", e));
        }
    };
    
    let streams = match json.get("streams").and_then(|s| s.as_array()) {
        Some(streams) => streams,
        None => {
            error!("Invalid JSON structure: no streams array found");
            error!("JSON output: {}", String::from_utf8_lossy(&ffprobe_result.stdout));
            return Err(anyhow!("Invalid JSON structure: no streams array"));
        }
    };
    
    info!("Found {} streams in video file", streams.len());
    
    // Debug log all streams
    for (i, stream) in streams.iter().enumerate() {
        let codec_type = stream.get("codec_type").and_then(|t| t.as_str()).unwrap_or("unknown");
        let codec_name = stream.get("codec_name").and_then(|t| t.as_str()).unwrap_or("unknown");
        info!("Stream #{}: type={}, codec={}", i, codec_type, codec_name);
    }
    
    let video_index = match streams.iter()
        .position(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("video")) {
        Some(idx) => {
            info!("Found video stream at index {}", idx);
            idx
        },
        None => {
            error!("No video stream found in input file");
            return Err(anyhow!("No video stream found"));
        }
    };
    
    // Убираем проверку аудио потока, так как видео намеренно скачивается без аудио
    info!("Skipping audio stream check in video file as it's downloaded without audio");
    
    // Проверяем аудиофайл до начала обработки
    info!("Checking translated audio file details");
    match tokio::fs::metadata(translated_audio_path).await {
        Ok(metadata) => {
            info!("Translated audio file size: {} bytes", metadata.len());
            if metadata.len() == 0 {
                error!("Translated audio file is empty (zero bytes)");
                return Err(anyhow!("Translated audio file is empty (zero bytes)"));
            }
        },
        Err(e) => {
            error!("Failed to get translated audio file metadata: {}", e);
            return Err(anyhow!("Failed to get translated audio file metadata: {}", e));
        }
    };

    // Получаем информацию о формате аудиофайла
    info!("Analyzing translated audio file format");
    let ffprobe_audio_args = [
        "-v", "error",
        "-print_format", "json",
        "-show_streams",
        translated_audio_path.to_str().unwrap_or("<invalid path>"),
    ];

    let full_audio_cmd = format!("ffprobe {}", ffprobe_audio_args.join(" "));
    info!("Running ffprobe command for audio: {}", full_audio_cmd);

    let ffprobe_audio_cmd = Command::new(&ffprobe_path)
        .args(&ffprobe_audio_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let ffprobe_audio_cmd = match ffprobe_audio_cmd {
        Ok(cmd) => cmd,
        Err(e) => {
            error!("Failed to spawn ffprobe command for audio: {}", e);
            return Err(anyhow!("Failed to spawn ffprobe for audio: {}", e));
        }
    };

    let ffprobe_audio_result = match timeout(Duration::from_secs(30), async {
        match ffprobe_audio_cmd.wait_with_output() {
            Ok(output) => Ok(output),
            Err(e) => Err(anyhow!("ffprobe process for audio failed: {}", e))
        }
    }).await {
        Ok(result) => result,
        Err(_) => {
            error!("ffprobe for audio timed out after 30 seconds");
            return Err(anyhow!("ffprobe for audio timed out after 30 seconds"));
        }
    }?;

    if !ffprobe_audio_result.status.success() {
        let stdout = String::from_utf8_lossy(&ffprobe_audio_result.stdout);
        let stderr = String::from_utf8_lossy(&ffprobe_audio_result.stderr);
        error!("ffprobe for audio failed with status: {}", ffprobe_audio_result.status);
        error!("ffprobe audio stdout: {}", stdout);
        error!("ffprobe audio stderr: {}", stderr);
        error!("Audio file path: {}", translated_audio_path.display());
        
        if !translated_audio_path.exists() {
            error!("Translated audio file does not exist at the time of ffprobe execution!");
            return Err(anyhow!("Translated audio file not found at execution time: {}", translated_audio_path.display()));
        }
        
        return Err(anyhow!("Failed to get audio metadata. Status: {}. Error: {}", 
                          ffprobe_audio_result.status, stderr));
    }

    // Логируем информацию о форматах обоих файлов
    info!("Both video and audio files validated successfully");

    // Попробуем проверить доступность команды ffmpeg перед началом конвертации субтитров
    info!("Checking ffmpeg availability");
    match which::which("ffmpeg") {
        Ok(path) => info!("Using ffmpeg from path: {}", path.display()),
        Err(e) => {
            error!("Failed to locate ffmpeg executable: {}", e);
            return Err(anyhow!("ffmpeg not found in PATH: {}", e));
        }
    };
    
    // Проверяем файлы субтитров
    info!("Checking subtitle files");
    for (name, path) in [("Original VTT", original_vtt_path), ("Translated VTT", translated_vtt_path)] {
        match tokio::fs::metadata(path).await {
            Ok(metadata) => {
                info!("{} file size: {} bytes", name, metadata.len());
                if metadata.len() == 0 {
                    error!("{} file is empty (zero bytes)", name);
                    return Err(anyhow!("{} file is empty (zero bytes)", name));
                }
            },
            Err(e) => {
                error!("Failed to get {} file metadata: {}", name, e);
                return Err(anyhow!("Failed to get {} file metadata: {}", name, e));
            }
        };
        
        // Проверяем содержимое файлов VTT
        match tokio::fs::read_to_string(path).await {
            Ok(content) => {
                let line_count = content.lines().count();
                info!("{} file contains {} lines", name, line_count);
                if line_count < 3 {
                    error!("{} file appears to be incomplete (only {} lines)", name, line_count);
                    info!("{} file content preview: {}", name, content);
                    return Err(anyhow!("{} file appears to be incomplete or invalid", name));
                }
            },
            Err(e) => {
                error!("Failed to read {} file content: {}", name, e);
                return Err(anyhow!("Failed to read {} file content: {}", name, e));
            }
        }
    }

    // Подробно логируем процесс конвертации субтитров
    info!("Starting subtitle conversion process");

    if let Some(sender) = &progress_sender {
        info!("Sending progress update: 40%");
        if let Err(e) = sender
            .send(MergeProgress {
                status: "Converting subtitles to .ass format".to_string(),
                progress: 40.0,
            })
            .await
        {
            warn!("Failed to send progress: {}", e);
        }
    }
    
    let original_ass_path = output_dir.join(format!("{}_original.ass", filename));
    let translated_ass_path = output_dir.join(format!("{}_translated.ass", filename));
    
    info!("Converting original subtitles from VTT to ASS...");
    info!("Source: {}", original_vtt_path.display());
    info!("Target: {}", original_ass_path.display());
    
    let vtt_to_ass_orig_start = Instant::now();
    
    // Convert original VTT to ASS with explicit timeout
    let vtt_to_ass_orig_cmd = Command::new("ffmpeg")
        .args([
            "-i", original_vtt_path.to_str().unwrap(),
            "-y", original_ass_path.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    
    let vtt_to_ass_orig_cmd = match vtt_to_ass_orig_cmd {
        Ok(cmd) => cmd,
        Err(e) => {
            error!("Failed to spawn ffmpeg for original subtitle conversion: {}", e);
            return Err(anyhow!("Failed to start original subtitle conversion: {}", e));
        }
    };
    
    // Wait with timeout
    let vtt_to_ass_orig_result = match timeout(Duration::from_secs(30), async {
        match vtt_to_ass_orig_cmd.wait_with_output() {
            Ok(output) => Ok(output),
            Err(e) => Err(anyhow!("ffmpeg failed: {}", e))
        }
    }).await {
        Ok(result) => result,
        Err(_) => {
            error!("Original subtitle conversion timed out after 30 seconds");
            return Err(anyhow!("Original subtitle conversion timed out"));
        }
    }?;
    
    info!("Original subtitle conversion completed in {:?}", vtt_to_ass_orig_start.elapsed());
    
    if !vtt_to_ass_orig_result.status.success() {
        let stderr = String::from_utf8_lossy(&vtt_to_ass_orig_result.stderr);
        error!("Original subtitle conversion failed with status: {}, stderr: {}", 
               vtt_to_ass_orig_result.status, stderr);
        return Err(anyhow!("Failed to convert original subtitles to ASS format: {}", stderr));
    }
    
    info!("Converting translated subtitles from VTT to ASS...");
    info!("Source: {}", translated_vtt_path.display());
    info!("Target: {}", translated_ass_path.display());
    
    let vtt_to_ass_trans_start = Instant::now();
    
    // Convert translated VTT to ASS with explicit timeout
    let vtt_to_ass_trans_cmd = Command::new("ffmpeg")
        .args([
            "-i", translated_vtt_path.to_str().unwrap(),
            "-y", translated_ass_path.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    
    let vtt_to_ass_trans_cmd = match vtt_to_ass_trans_cmd {
        Ok(cmd) => cmd,
        Err(e) => {
            error!("Failed to spawn ffmpeg for translated subtitle conversion: {}", e);
            return Err(anyhow!("Failed to start translated subtitle conversion: {}", e));
        }
    };
    
    // Wait with timeout
    let vtt_to_ass_trans_result = match timeout(Duration::from_secs(30), async {
        match vtt_to_ass_trans_cmd.wait_with_output() {
            Ok(output) => Ok(output),
            Err(e) => Err(anyhow!("ffmpeg failed: {}", e))
        }
    }).await {
        Ok(result) => result,
        Err(_) => {
            error!("Translated subtitle conversion timed out after 30 seconds");
            return Err(anyhow!("Translated subtitle conversion timed out"));
        }
    }?;
    
    info!("Translated subtitle conversion completed in {:?}", vtt_to_ass_trans_start.elapsed());
    
    if !vtt_to_ass_trans_result.status.success() {
        let stderr = String::from_utf8_lossy(&vtt_to_ass_trans_result.stderr);
        error!("Translated subtitle conversion failed with status: {}, stderr: {}", 
               vtt_to_ass_trans_result.status, stderr);
        return Err(anyhow!("Failed to convert translated subtitles to ASS format: {}", stderr));
    }
    
    // Check if ASS files were actually created
    if !original_ass_path.exists() {
        error!("Original ASS file was not created even though ffmpeg reported success");
        return Err(anyhow!("Original ASS file was not created"));
    }
    
    if !translated_ass_path.exists() {
        error!("Translated ASS file was not created even though ffmpeg reported success");
        return Err(anyhow!("Translated ASS file was not created"));
    }
    
    // Добавляем проверку размера выходных ASS файлов
    for (name, path) in [("Original ASS", &original_ass_path), ("Translated ASS", &translated_ass_path)] {
        match tokio::fs::metadata(path).await {
            Ok(metadata) => {
                info!("{} file size: {} bytes", name, metadata.len());
                if metadata.len() < 100 {  // Обычно ASS файлы имеют хотя бы заголовок и должны быть больше 100 байт
                    error!("{} file is suspiciously small: {} bytes", name, metadata.len());
                    // Проверяем содержимое
                    if let Ok(content) = tokio::fs::read_to_string(path).await {
                        error!("{} file content: {}", name, content);
                    }
                }
            },
            Err(e) => {
                error!("Failed to get {} file metadata after conversion: {}", name, e);
            }
        };
    }
    
    // Update progress
    if let Some(sender) = &progress_sender {
        info!("Sending progress update: 60%");
        if let Err(e) = sender
            .send(MergeProgress {
                status: "Running final ffmpeg merge".to_string(),
                progress: 60.0,
            })
            .await
        {
            warn!("Failed to send progress: {}", e);
        }
    }
    
    // Улучшаем код финального объединения
    info!("Preparing final merge command");

    // Подробно логируем аргументы командной строки ffmpeg
    let ffmpeg_args = [
        "-i", video_path.to_str().unwrap_or("<invalid path>"),             // Input 0: Video without audio
        "-i", translated_audio_path.to_str().unwrap_or("<invalid path>"),  // Input 1: Translated audio
        "-i", original_audio_path.to_str().unwrap_or("<invalid path>"),    // Input 2: Original audio
        "-i", original_ass_path.to_str().unwrap_or("<invalid path>"),      // Input 3: Original subtitles
        "-i", translated_ass_path.to_str().unwrap_or("<invalid path>"),    // Input 4: Translated subtitles
        "-map", "0:v",                                            // Video from input 0
        "-map", "1:a",                                            // Translated audio from input 1
        "-map", "2:a",                                            // Original audio from input 2
        "-map", "3",                                              // Original subtitles from input 3
        "-map", "4",                                              // Translated subtitles from input 4
        "-c:v", "copy",                                           // Copy video codec
        "-c:a", "copy",                                           // Copy audio codec
        "-c:s", "copy",                                           // Copy subtitle codec
        "-metadata:s:a:0", "title=Translated Audio",              // Label translated audio
        "-metadata:s:a:1", "title=Original Audio",                // Label original audio
        "-metadata:s:s:0", "title=Original Subtitles",            // Label original subs
        "-metadata:s:s:1", "title=Translated Subtitles",          // Label translated subs
        "-disposition:a:0", "default",                            // Set translated audio as default
        "-disposition:s:1", "default",                            // Set translated subs as default
        "-y", output_file.to_str().unwrap_or("<invalid path>")    // Output file
    ];
    
    // Log the full ffmpeg command
    let ffmpeg_cmd_str = ffmpeg_args.join(" ");
    info!("Executing final ffmpeg command: ffmpeg {}", ffmpeg_cmd_str);
    
    let final_ffmpeg_start = Instant::now();
    
    // Execute the final ffmpeg command with timeout monitoring
    info!("Spawning final ffmpeg process...");
    let final_ffmpeg_cmd = Command::new("ffmpeg")
        .args(&ffmpeg_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    
    let mut final_ffmpeg_cmd = match final_ffmpeg_cmd {
        Ok(cmd) => cmd,
        Err(e) => {
            error!("Failed to spawn final ffmpeg command: {}", e);
            return Err(anyhow!("Failed to start final ffmpeg merge: {}", e));
        }
    };
    
    // Start monitoring the ffmpeg process
    let pid = final_ffmpeg_cmd.id();
    info!("Started final ffmpeg process with PID: {}", pid);
    
    // Create a monitor for the ffmpeg process
    let monitor = Arc::new(Mutex::new(FfmpegMonitor {
        pid,
        is_stuck: false,
        last_activity: Instant::now(),
    }));
    
    // Spawn a task to monitor the ffmpeg process
    let monitor_clone = monitor.clone();
    let _monitor_handle = tokio::spawn(monitor_ffmpeg_process(pid, monitor_clone));
    
    // Wait with a longer timeout for the final merge
    info!("Waiting for ffmpeg to complete with 10 minute timeout...");
    let final_ffmpeg_result = match timeout(Duration::from_secs(600), async {
        match final_ffmpeg_cmd.wait_with_output() {
            Ok(output) => {
                let mut mon_guard = monitor.lock().await;
                mon_guard.last_activity = Instant::now();
                Ok(output)
            },
            Err(e) => Err(anyhow!("ffmpeg failed: {}", e))
        }
    }).await {
        Ok(result) => result,
        Err(_) => {
            error!("Final ffmpeg merge timed out after 600 seconds (10 minutes)");
            
            // Try to kill the process if it's still running
            #[cfg(target_family = "unix")]
            {
                if let Err(e) = std::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .output() 
                {
                    error!("Failed to kill ffmpeg process after timeout: {}", e);
                }
            }
            
            #[cfg(target_family = "windows")]
            {
                if let Err(e) = std::process::Command::new("taskkill")
                    .args(["/F", "/PID", &pid.to_string()])
                    .output() 
                {
                    error!("Failed to kill ffmpeg process after timeout: {}", e);
                }
            }
            
            return Err(anyhow!("Final ffmpeg merge timed out after 10 minutes"));
        }
    }?;
    
    // Check monitor status
    let monitor_status = monitor.lock().await;
    if monitor_status.is_stuck {
        warn!("ffmpeg process was detected as stuck during execution");
    }
    
    info!("Final ffmpeg merge completed in {:?}", final_ffmpeg_start.elapsed());
    
    // Process the ffmpeg output
    let exit_status = final_ffmpeg_result.status;
    let stderr = String::from_utf8_lossy(&final_ffmpeg_result.stderr).to_string();
    let stdout = String::from_utf8_lossy(&final_ffmpeg_result.stdout).to_string();
    
    if !exit_status.success() {
        error!("Final ffmpeg command failed with exit code: {}", exit_status);
        error!("ffmpeg stderr: {}", stderr);
        return Err(anyhow!("ffmpeg merge failed: {}", stderr));
    }
    
    // Log the execution time
    let ffmpeg_duration = final_ffmpeg_start.elapsed();
    info!("Final ffmpeg merge completed in {:.2?}", ffmpeg_duration);
    
    // Check if the output file exists and has content
    if let Ok(metadata) = tokio::fs::metadata(&output_file).await {
        let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
        info!("Output file exists with size: {:.2} MB", size_mb);
        if metadata.len() == 0 {
            warn!("Output file exists but has zero size!");
            return Err(anyhow!("Output file has zero size. Merge failed."));
        }
    } else {
        error!("Output file does not exist after ffmpeg completion!");
        return Err(anyhow!("Output file not found after merge completion"));
    }
    
    // Update progress to complete
    if let Some(sender) = &progress_sender {
        info!("Sending final progress update: 100%");
        if let Err(e) = sender
            .send(MergeProgress {
                status: "Merge complete".to_string(),
                progress: 100.0,
            })
            .await
        {
            warn!("Failed to send final progress: {}", e);
        }
    }
    
    // Attempt to delete temporary ASS files
    info!("Cleaning up temporary ASS files");
    if let Err(e) = std::fs::remove_file(&original_ass_path) {
        warn!("Failed to delete temporary original ASS file: {}", e);
    }
    
    if let Err(e) = std::fs::remove_file(&translated_ass_path) {
        warn!("Failed to delete temporary translated ASS file: {}", e);
    }
    
    let total_time = start_time.elapsed();
    info!("===== MERGE PROCESS COMPLETED in {:?} =====", total_time);
    info!("Final output file: {}", output_file.display());
    
    info!("Media merge completed successfully!");
    
    // Return success
    Ok(output_file)
} 