use super::models::{FishSpeechError, FishSpeechResult, TtsRequest, TtsResponse, Voice, SpeechFormat};
use super::config;
use reqwest::{Client, multipart};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::process::Command as TokioCommand;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use std::time::{SystemTime, UNIX_EPOCH};
use once_cell::sync::Lazy;

// API client instance
static API_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .expect("Failed to create HTTP client")
});

// API server instance
static API_SERVER: Lazy<Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>> = Lazy::new(|| {
    Arc::new(tokio::sync::Mutex::new(None))
});

// API connection info
#[derive(Debug, Clone)]
struct ApiConnection {
    url: String,
    port: u16,
    is_local: bool,
}

/// Get the API URL
async fn get_api_connection() -> FishSpeechResult<ApiConnection> {
    let config = config::get_config()?;
    
    if let Some(endpoint) = &config.api_endpoint {
        return Ok(ApiConnection {
            url: endpoint.clone(),
            port: 0, // Port is included in the endpoint
            is_local: false,
        });
    }
    
    // Default to local API
    let port = config.api_port.unwrap_or(7860);
    
    Ok(ApiConnection {
        url: format!("http://127.0.0.1:{}", port),
        port,
        is_local: true,
    })
}

/// Start the API server
pub async fn start_api_server() -> FishSpeechResult<()> {
    let mut server_guard = API_SERVER.lock().await;
    
    // Check if server is already running
    if let Some(child) = &mut *server_guard {
        if let Ok(None) = child.try_wait() {
            // Server is already running
            return Ok(());
        }
    }
    
    // Start a new server
    let config = config::get_config()?;
    let install_path = config.install_path.clone();
    
    let api_connection = get_api_connection().await?;
    
    if !api_connection.is_local {
        // Using remote API, no need to start a server
        return Ok(());
    }
    
    log::info!("Starting Fish Speech API server...");
    
    let python_exec = if Path::new(&install_path).join("venv").exists() {
        if cfg!(target_os = "windows") {
            install_path.join("venv/Scripts/python.exe")
        } else {
            install_path.join("venv/bin/python")
        }
    } else {
        // Assume conda
        if cfg!(target_os = "windows") {
            PathBuf::from("conda.exe")
        } else {
            PathBuf::from("conda")
        }
    };
    
    let mut cmd = if python_exec.file_name().unwrap() == "conda" || python_exec.file_name().unwrap() == "conda.exe" {
        let mut cmd = TokioCommand::new(python_exec);
        cmd.args(["run", "-n", "fish-speech", "python", "tools/run_webui.py"]);
        cmd
    } else {
        let mut cmd = TokioCommand::new(&python_exec);
        cmd.arg("tools/run_webui.py");
        cmd
    };
    
    // Configure API options
    cmd.args([
        "--api",
        "--listen", "127.0.0.1",
        "--port", &api_connection.port.to_string(),
    ]);
    
    // Add device flag if set
    if config.device != "auto" {
        cmd.args(["--device", &config.device]);
    }
    
    // Run in the installation directory
    cmd.current_dir(&install_path);
    
    // Capture stdout and stderr
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    
    // Start the process
    let mut child = cmd.spawn()
        .map_err(|e| FishSpeechError::InstallationError(format!("Failed to start API server: {}", e)))?;
    
    // Read stdout and stderr for logging
    if let Some(stdout) = child.stdout.take() {
        let stdout_reader = BufReader::new(stdout);
        tokio::spawn(async move {
            let mut lines = stdout_reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log::info!("Fish Speech API: {}", line);
            }
        });
    }
    
    if let Some(stderr) = child.stderr.take() {
        let stderr_reader = BufReader::new(stderr);
        tokio::spawn(async move {
            let mut lines = stderr_reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log::error!("Fish Speech API: {}", line);
            }
        });
    }
    
    // Wait for API to start
    let api_url = format!("{}/v1/health", api_connection.url);
    
    for _ in 0..30 {
        // Try to connect to the API
        match reqwest::get(&api_url).await {
            Ok(response) => {
                if response.status().is_success() {
                    log::info!("Fish Speech API server started successfully");
                    *server_guard = Some(child);
                    return Ok(());
                }
            }
            Err(_) => {
                // API not ready yet, wait and retry
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }
    }
    
    // Fail if API didn't start after timeout
    let _ = child.kill().await;
    Err(FishSpeechError::InstallationError("Failed to start API server: timeout".to_string()))
}

/// Stop the API server
pub async fn stop_api_server() -> FishSpeechResult<()> {
    let mut server_guard = API_SERVER.lock().await;
    
    if let Some(child) = &mut *server_guard {
        // Try to terminate gracefully
        if let Err(e) = child.kill().await {
            log::warn!("Failed to kill API server: {}", e);
        }
        
        // Remove from global state
        *server_guard = None;
    }
    
    Ok(())
}

/// Check if API server is running
pub async fn is_api_server_running() -> bool {
    let server_guard = API_SERVER.lock().await;
    
    if let Some(child) = &*server_guard {
        return child.id().is_some();
    }
    
    false
}

/// List available voices
pub async fn list_voices() -> FishSpeechResult<Vec<Voice>> {
    let api_connection = get_api_connection().await?;
    
    if !is_api_server_running().await && api_connection.is_local {
        start_api_server().await?;
    }
    
    let url = format!("{}/v1/voices", api_connection.url);
    
    let response = API_CLIENT.get(&url)
        .send()
        .await
        .map_err(|e| FishSpeechError::NetworkError(e.to_string()))?;
    
    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(FishSpeechError::NetworkError(format!("API request failed with status {}: {}", status, error_text)));
    }
    
    #[derive(Deserialize)]
    struct VoiceResponse {
        voices: Vec<Voice>,
    }
    
    let voice_response: VoiceResponse = response.json()
        .await
        .map_err(|e| FishSpeechError::NetworkError(format!("Failed to parse API response: {}", e)))?;
    
    Ok(voice_response.voices)
}

/// Generate speech using Fish Speech
pub async fn generate_speech(request: TtsRequest) -> FishSpeechResult<TtsResponse> {
    let config = config::get_config()?;
    let api_connection = get_api_connection().await?;
    
    if !is_api_server_running().await && api_connection.is_local {
        start_api_server().await?;
    }
    
    // Create output directory if it doesn't exist
    if !config.output_path.exists() {
        std::fs::create_dir_all(&config.output_path)
            .map_err(|e| FishSpeechError::IoError(e))?;
    }
    
    // Generate a unique output filename
    let output_file = config.output_path.join(format!(
        "speech_{}.{}",
        generate_unique_id(),
        match request.format {
            SpeechFormat::Wav => "wav",
            SpeechFormat::Mp3 => "mp3",
            SpeechFormat::Ogg => "ogg",
        }
    ));
    
    let url = format!("{}/v1/tts", api_connection.url);
    
    #[derive(Serialize)]
    struct ApiRequest {
        text: String,
        voice_id: String,
        format: String,
        rate: f32,
    }
    
    let api_request = ApiRequest {
        text: request.text.clone(),
        voice_id: request.voice_id.clone(),
        format: match request.format {
            SpeechFormat::Wav => "wav",
            SpeechFormat::Mp3 => "mp3",
            SpeechFormat::Ogg => "ogg",
        }.to_string(),
        rate: request.rate,
    };
    
    if request.stream {
        // For streaming mode, use chunked transfer
        // This is more complex and requires handling streaming response
        // Simplified implementation for now - in real code you'd handle streaming properly
        log::warn!("Streaming mode requested but not fully implemented, falling back to non-streaming mode");
    }
    
    // Send request
    let response = API_CLIENT.post(&url)
        .json(&api_request)
        .send()
        .await
        .map_err(|e| FishSpeechError::NetworkError(e.to_string()))?;
    
    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(FishSpeechError::NetworkError(format!("API request failed with status {}: {}", status, error_text)));
    }
    
    // Save the audio response to the output file
    let bytes = response.bytes()
        .await
        .map_err(|e| FishSpeechError::NetworkError(e.to_string()))?;
    
    let mut file = File::create(&output_file)
        .await
        .map_err(|e| FishSpeechError::IoError(e))?;
    
    file.write_all(&bytes)
        .await
        .map_err(|e| FishSpeechError::IoError(e))?;
    
    // Get audio duration (this would need to be implemented based on the audio format)
    let duration = get_audio_duration(&output_file).await?;
    
    // Get current timestamp
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    
    Ok(TtsResponse {
        audio_path: output_file,
        duration,
        format: request.format,
        timestamp,
    })
}

/// Stop speech generation
pub async fn stop_generation() -> FishSpeechResult<()> {
    let api_connection = get_api_connection().await?;
    
    if !is_api_server_running().await {
        // No active generation to stop
        return Ok(());
    }
    
    let url = format!("{}/v1/stop", api_connection.url);
    
    let response = API_CLIENT.post(&url)
        .send()
        .await
        .map_err(|e| FishSpeechError::NetworkError(e.to_string()))?;
    
    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(FishSpeechError::NetworkError(format!("API request failed with status {}: {}", status, error_text)));
    }
    
    Ok(())
}

/// Get audio duration using ffmpeg
async fn get_audio_duration(file_path: &Path) -> FishSpeechResult<f32> {
    // Using ffprobe to get audio duration
    let output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            file_path.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| FishSpeechError::IoError(e))?;
    
    if !output.status.success() {
        // Fallback to a default value if ffprobe fails
        return Ok(0.0);
    }
    
    let duration_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let duration: f32 = duration_str.parse().unwrap_or(0.0);
    
    Ok(duration)
}

// Вместо uuid используем простую функцию для генерации уникального ID на основе времени
fn generate_unique_id() -> String {
    let now = SystemTime::now();
    let since_epoch = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let millis = since_epoch.as_millis();
    let nanos = since_epoch.subsec_nanos();
    
    format!("{}_{}", millis, nanos)
} 