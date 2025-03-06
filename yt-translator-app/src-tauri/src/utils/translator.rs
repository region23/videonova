use crate::models::{ProcessStatus, ProgressUpdate, TranslationRequest};
use crate::utils::tools::get_tool_path;
use anyhow::{anyhow, Context, Result};
use log::{debug, error};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Structure representing the translation process
pub struct Translator {
    /// Translation request parameters
    request: TranslationRequest,
    /// Sender for progress updates
    progress_sender: mpsc::Sender<ProgressUpdate>,
    /// Work directory for temporary files
    work_dir: PathBuf,
    /// UUID for the current process
    process_id: String,
}

impl Translator {
    /// Create a new translator instance
    pub fn new(
        request: TranslationRequest,
        progress_sender: mpsc::Sender<ProgressUpdate>,
    ) -> Result<Self> {
        // Create a temporary directory for work files
        let sys_temp = std::env::temp_dir();
        let process_id = Uuid::new_v4().to_string();
        let work_dir = sys_temp.join("youtube-translator").join(&process_id);
        fs::create_dir_all(&work_dir)?;

        Ok(Self {
            request,
            progress_sender,
            work_dir,
            process_id,
        })
    }

    /// Start the translation process
    pub async fn start(&self) -> Result<String> {
        // Only show progress for demonstration
        
        // Step 1: Download video
        self.send_progress(ProcessStatus::Downloading, 0.0, None).await?;
        // Simulate downloading (sleep for 1 second)
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        self.send_progress(ProcessStatus::Downloading, 100.0, None).await?;
        
        // Step 2: Recognize speech
        self.send_progress(ProcessStatus::Recognizing, 0.0, None).await?;
        // Simulate speech recognition (sleep for 1 second)
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        self.send_progress(ProcessStatus::Recognizing, 100.0, None).await?;
        
        // Step 3: Translate subtitles
        self.send_progress(ProcessStatus::Translating, 0.0, None).await?;
        // Simulate translation (sleep for 1 second)
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        self.send_progress(ProcessStatus::Translating, 100.0, None).await?;
        
        // Step 4: Generate speech
        self.send_progress(ProcessStatus::GeneratingSpeech, 0.0, None).await?;
        // Simulate speech generation (sleep for 1 second)
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        self.send_progress(ProcessStatus::GeneratingSpeech, 100.0, None).await?;
        
        // Step 5: Merge media
        self.send_progress(ProcessStatus::Merging, 0.0, None).await?;
        // Simulate merging (sleep for 1 second)
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        
        // Create a fake output path in the user's output directory
        let output_filename = "translated_video.mp4";
        let output_path = PathBuf::from(&self.request.output_directory).join(output_filename);
        
        self.send_progress(ProcessStatus::Merging, 100.0, None).await?;
        
        // Process completed
        self.send_progress(
            ProcessStatus::Completed, 
            100.0, 
            Some(output_path.to_string_lossy().to_string())
        ).await?;
        
        // Return output path
        Ok(output_path.to_string_lossy().to_string())
    }

    /// Download video from YouTube
    async fn download_video(&self) -> Result<PathBuf> {
        let yt_dlp_path = get_tool_path("yt-dlp")
            .ok_or_else(|| anyhow!("yt-dlp not found"))?;

        // Prepare output path
        let video_filename = format!("{}-video.mp4", self.process_id);
        let video_path = self.work_dir.join(&video_filename);

        // Build download command
        let mut command = Command::new(&yt_dlp_path);
        command
            .arg("-f")
            .arg("bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best")
            .arg("-o")
            .arg(&video_path)
            .arg(&self.request.youtube_url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!("Running yt-dlp command: {:?}", command);

        // Execute command
        let output = command.output().context("Failed to execute yt-dlp")?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            error!("yt-dlp error: {}", error_msg);
            return Err(anyhow!("Failed to download video: {}", error_msg));
        }

        if !video_path.exists() {
            return Err(anyhow!("Video file was not created"));
        }

        Ok(video_path)
    }

    /// Extract audio from video
    async fn extract_audio(&self, video_path: &Path) -> Result<PathBuf> {
        let ffmpeg_path = get_tool_path("ffmpeg")
            .ok_or_else(|| anyhow!("ffmpeg not found"))?;

        // Prepare output path
        let audio_filename = format!("{}-audio.wav", self.process_id);
        let audio_path = self.work_dir.join(&audio_filename);

        // Build extract command
        let mut command = Command::new(&ffmpeg_path);
        command
            .arg("-i")
            .arg(video_path)
            .arg("-vn")
            .arg("-acodec")
            .arg("pcm_s16le")
            .arg("-ar")
            .arg("16000")
            .arg("-ac")
            .arg("1")
            .arg("-y")
            .arg(&audio_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!("Running ffmpeg extract command: {:?}", command);

        // Execute command
        let output = command.output().context("Failed to execute ffmpeg")?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            error!("ffmpeg error: {}", error_msg);
            return Err(anyhow!("Failed to extract audio: {}", error_msg));
        }

        if !audio_path.exists() {
            return Err(anyhow!("Audio file was not created"));
        }

        Ok(audio_path)
    }

    /// Recognize speech using Whisper
    async fn recognize_speech(&self, _audio_path: &Path) -> Result<PathBuf> {
        // TODO: Implement actual integration with OpenAI Whisper API
        // For now, we'll use a placeholder that just creates an empty subtitles file

        let subtitles_filename = format!("{}-subtitles.vtt", self.process_id);
        let subtitles_path = self.work_dir.join(&subtitles_filename);

        // Placeholder - in real implementation, this would call OpenAI API
        let dummy_subtitles = "WEBVTT\n\n00:00:00.000 --> 00:00:05.000\nThis is a placeholder for subtitles.\n\n";
        fs::write(&subtitles_path, dummy_subtitles)?;

        Ok(subtitles_path)
    }

    /// Translate subtitles using OpenAI
    async fn translate_subtitles(&self, _subtitles_path: &Path) -> Result<PathBuf> {
        // TODO: Implement actual integration with OpenAI translation
        // For now, we'll use a placeholder that just creates a "translated" subtitles file

        let translated_filename = format!("{}-translated.vtt", self.process_id);
        let translated_path = self.work_dir.join(&translated_filename);

        // Placeholder - in real implementation, this would call OpenAI API
        let dummy_translation = "WEBVTT\n\n00:00:00.000 --> 00:00:05.000\nThis is a placeholder for translated subtitles.\n\n";
        fs::write(&translated_path, dummy_translation)?;

        Ok(translated_path)
    }

    /// Generate speech from text
    async fn generate_speech(&self, _subtitles_path: &Path) -> Result<PathBuf> {
        // TODO: Implement actual integration with text-to-speech API
        // For now, we'll use a placeholder that just creates a silent audio file

        let tts_filename = format!("{}-tts.wav", self.process_id);
        let tts_path = self.work_dir.join(&tts_filename);

        // Generate silent audio using ffmpeg
        let ffmpeg_path = get_tool_path("ffmpeg")
            .ok_or_else(|| anyhow!("ffmpeg not found"))?;

        let mut command = Command::new(&ffmpeg_path);
        command
            .arg("-f")
            .arg("lavfi")
            .arg("-i")
            .arg("anullsrc=r=16000:cl=mono")
            .arg("-t")
            .arg("5") // 5 seconds of silence
            .arg("-y")
            .arg(&tts_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!("Running ffmpeg TTS placeholder command: {:?}", command);

        // Execute command
        let output = command.output().context("Failed to execute ffmpeg")?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            error!("ffmpeg error: {}", error_msg);
            return Err(anyhow!("Failed to create TTS audio: {}", error_msg));
        }

        if !tts_path.exists() {
            return Err(anyhow!("TTS audio file was not created"));
        }

        Ok(tts_path)
    }

    /// Merge video, audio and subtitles
    async fn merge_media(&self, video_path: &Path, audio_path: &Path, subtitles_path: &Path) -> Result<PathBuf> {
        let ffmpeg_path = get_tool_path("ffmpeg")
            .ok_or_else(|| anyhow!("ffmpeg not found"))?;

        // Prepare output filename - use original file name + translation info
        let video_filename = video_path.file_name()
            .ok_or_else(|| anyhow!("Invalid video filename"))?
            .to_string_lossy();

        // Extract the base name without extension
        let re = Regex::new(r"^(.*?)(?:-video)?\.(?:mp4|mkv|webm)$")?;
        let output_base_name = if let Some(caps) = re.captures(&video_filename) {
            caps.get(1).map_or("video", |m| m.as_str())
        } else {
            "video"
        };

        // Create final output filename
        let source_lang = if self.request.source_language == "auto" {
            "auto".to_string()
        } else {
            self.request.source_language.clone()
        };

        let output_filename = format!(
            "{}_{}_{}.mp4",
            output_base_name,
            source_lang,
            self.request.target_language
        );

        // Create full output path
        let output_dir = PathBuf::from(&self.request.output_directory);
        let output_path = output_dir.join(&output_filename);

        // Build merge command
        let mut command = Command::new(&ffmpeg_path);
        command
            .arg("-i")
            .arg(video_path)
            .arg("-i")
            .arg(audio_path)
            .arg("-i")
            .arg(subtitles_path)
            .arg("-map")
            .arg("0:v:0")  // First input (video), first video stream
            .arg("-map")
            .arg("1:a:0")  // Second input (tts audio), first audio stream
            .arg("-c:v")
            .arg("copy")   // Copy video codec
            .arg("-c:a")
            .arg("aac")    // Convert audio to AAC
            .arg("-b:a")
            .arg("192k")   // Audio bitrate
            .arg("-c:s")
            .arg("mov_text") // Subtitle codec
            .arg("-y")     // Overwrite if exists
            .arg(&output_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!("Running ffmpeg merge command: {:?}", command);

        // Execute command
        let output = command.output().context("Failed to execute ffmpeg")?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            error!("ffmpeg error: {}", error_msg);
            return Err(anyhow!("Failed to merge media: {}", error_msg));
        }

        if !output_path.exists() {
            return Err(anyhow!("Output file was not created"));
        }

        Ok(output_path)
    }

    /// Send progress update
    async fn send_progress(&self, status: ProcessStatus, progress: f32, output_file: Option<String>) -> Result<()> {
        self.progress_sender
            .send(ProgressUpdate {
                status,
                progress,
                message: None,
                output_file,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress update: {}", e))
    }
} 