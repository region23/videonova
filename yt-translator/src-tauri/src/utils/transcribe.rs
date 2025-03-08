use anyhow::{anyhow, Result};
use log::{debug, info, warn, error};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_util::codec::{BytesCodec, FramedRead};
use crate::utils::common::{sanitize_filename, check_file_exists_and_valid};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranscriptionProgress {
    pub status: String,
    pub progress: f32,
}

// Создание формы для запроса к OpenAI API
pub enum ResponseFormat {
    Json,
    Text,
    Srt,
    VerboseJson,
    Vtt,
}

impl Default for ResponseFormat {
    fn default() -> Self {
        ResponseFormat::Vtt
    }
}

impl ToString for ResponseFormat {
    fn to_string(&self) -> String {
        match self {
            ResponseFormat::Json => "json".to_string(),
            ResponseFormat::Text => "text".to_string(),
            ResponseFormat::Srt => "srt".to_string(),
            ResponseFormat::VerboseJson => "verbose_json".to_string(),
            ResponseFormat::Vtt => "vtt".to_string(),
        }
    }
}

#[derive(Debug)]
struct MultipartFormBuilder {
    boundary: String,
    body: Vec<u8>,
}

impl MultipartFormBuilder {
    const DEFAULT_BOUNDARY: &'static str = "--------------------boundary";

    fn new() -> Self {
        Self {
            boundary: Self::DEFAULT_BOUNDARY.to_string(),
            body: Vec::new(),
        }
    }

    fn with_boundary(boundary: &str) -> Self {
        Self {
            boundary: boundary.to_string(),
            body: Vec::new(),
        }
    }

    fn add_text(&mut self, name: &str, value: &str) -> &mut Self {
        self.body.extend_from_slice(format!("--{}\r\n", self.boundary).as_bytes());
        self.body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes());
        self.body.extend_from_slice(value.as_bytes());
        self.body.extend_from_slice(b"\r\n");
        self
    }

    fn add_file(&mut self, name: &str, filename: &str, content: &[u8], content_type: &str) -> &mut Self {
        self.body.extend_from_slice(format!("--{}\r\n", self.boundary).as_bytes());
        self.body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                name, filename
            )
            .as_bytes(),
        );
        self.body.extend_from_slice(format!("Content-Type: {}\r\n\r\n", content_type).as_bytes());
        self.body.extend_from_slice(content);
        self.body.extend_from_slice(b"\r\n");
        self
    }

    fn finish(&mut self) -> Vec<u8> {
        self.body.extend_from_slice(format!("--{}--\r\n", self.boundary).as_bytes());
        std::mem::take(&mut self.body)
    }

    fn content_type(&self) -> String {
        format!("multipart/form-data; boundary={}", self.boundary)
    }

    
}

pub async fn transcribe_audio(
    audio_path: &Path,
    output_dir: &Path,
    api_key: &str,
    language: Option<String>,
    progress_sender: Option<mpsc::Sender<TranscriptionProgress>>,
) -> Result<PathBuf> {
    info!("Starting transcription process for file: {}", audio_path.display());
    
    // Validate API key
    if api_key.trim().is_empty() {
        error!("OpenAI API key is empty");
        return Err(anyhow!("OpenAI API key is required for transcription"));
    }
    
    debug!("Using OpenAI API key: {}...", if api_key.len() >= 4 { &api_key[..4] } else { "[invalid]" });
    if let Some(lang) = &language {
        debug!("Using language: {}", lang);
    }
    
    // Always use VTT format
    let format = ResponseFormat::Vtt;
    debug!("Using response format: {}", format.to_string());
    
    // Расширение выходного файла зависит от формата ответа
    let file_extension = match format {
        ResponseFormat::Json => "json",
        ResponseFormat::Text => "txt",
        ResponseFormat::Srt => "srt",
        ResponseFormat::VerboseJson => "json",
        ResponseFormat::Vtt => "vtt",
    };
    
    // Create output directory if it doesn't exist
    debug!("Creating output directory: {}", output_dir.display());
    match fs::create_dir_all(output_dir).await {
        Ok(_) => debug!("Output directory created or already exists"),
        Err(e) => {
            error!("Failed to create output directory: {}", e);
            return Err(anyhow!("Failed to create output directory: {}", e));
        }
    }
    
    // Проверяем существование файла
    debug!("Checking if audio file exists: {}", audio_path.display());
    if !audio_path.exists() {
        error!("Audio file does not exist: {}", audio_path.display());
        return Err(anyhow!("Audio file does not exist: {}", audio_path.display()));
    }
    
    // Проверяем права доступа к файлу
    debug!("Checking if audio file is accessible: {}", audio_path.display());
    let metadata = match std::fs::metadata(audio_path) {
        Ok(meta) => {
            debug!("File metadata retrieved successfully");
            meta
        },
        Err(e) => {
            error!("Failed to get file metadata: {}", e);
            return Err(anyhow!("Failed to get file metadata: {}", e));
        }
    };
    
    if !metadata.is_file() {
        error!("Path is not a file: {}", audio_path.display());
        return Err(anyhow!("Path is not a file: {}", audio_path.display()));
    }
    
    debug!("Opening audio file: {}", audio_path.display());
    
    // Define output file path (same name as input but with appropriate extension)
    let file_stem = audio_path
        .file_stem()
        .ok_or_else(|| anyhow!("Failed to get file stem"))?
        .to_string_lossy();
    
    // Обрабатываем имя файла - переводим в нижний регистр и заменяем пробелы на подчеркивания
    let sanitized_file_stem = sanitize_filename(&file_stem);
    debug!("Original file stem: {}, sanitized: {}", file_stem, sanitized_file_stem);
    
    let output_path = output_dir.join(format!("{}.{}", sanitized_file_stem, file_extension));
    debug!("Output will be saved to: {}", output_path.display());

    // Check if transcription file already exists
    if check_file_exists_and_valid(&output_path).await {
        info!("Found existing transcription file, skipping transcription");
        return Ok(output_path);
    }

    // Send initial progress
    if let Some(sender) = &progress_sender {
        sender
            .send(TranscriptionProgress {
                status: "Preparing transcription".to_string(),
                progress: 0.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }

    // Читаем файл целиком в память
    let file_content = match tokio::fs::read(audio_path).await {
        Ok(content) => {
            debug!("File read successfully, size: {} bytes", content.len());
            content
        },
        Err(e) => {
            error!("Failed to read audio file: {}", e);
            return Err(anyhow!("Failed to read audio file: {}", e));
        }
    };

    // Создаем multipart form-data с помощью builder'а
    let mut form = MultipartFormBuilder::new();
    let filename = audio_path.file_name().unwrap().to_string_lossy();
    
    // Добавляем все поля
    form.add_text("model", "whisper-1")
        .add_text("response_format", &format.to_string());

    // Добавляем язык если есть
    if let Some(lang) = &language {
        form.add_text("language", lang);
    }

    // Добавляем файл
    form.add_file("file", &filename, &file_content, "application/octet-stream");


    // Получаем финальное тело запроса
    let body = form.finish();
    
    // Send progress update - preparing the request
    if let Some(sender) = &progress_sender {
        sender
            .send(TranscriptionProgress {
                status: "Preparing request to OpenAI".to_string(),
                progress: 5.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }
    
    // Create the client and request
    debug!("Initializing HTTP client for OpenAI API");
    let client = reqwest::Client::new();
    
    // Отправляем запрос
    info!("Sending request to OpenAI Whisper API...");
    debug!("API endpoint: https://api.openai.com/v1/audio/transcriptions");
    debug!("Request Content-Type: {}", form.content_type());
    
    let response_result = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", form.content_type())
        .body(body)
        .send()
        .await;
    
    match response_result {
        Ok(response) => {
            let status = response.status();
            info!("OpenAI API response status: {}", status);
            debug!("Response headers: {:#?}", response.headers());
            
            // Send progress update
            if let Some(sender) = &progress_sender {
                sender
                    .send(TranscriptionProgress {
                        status: format!("Processing transcription result (HTTP {})", status.as_u16()),
                        progress: 90.0,
                    })
                    .await
                    .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
            }
            
            // Check if request was successful
            if !status.is_success() {
                let error_text = response.text().await?;
                error!("OpenAI API error: HTTP {}, body: {}", status, error_text);
                return Err(anyhow!("API request failed (HTTP {}): {}", status, error_text));
            }
            
            // Get response text
            let content = response.text().await?;
            debug!("Received content, length: {} bytes", content.len());
            if content.len() < 1000 {
                debug!("Content preview: {}", content);
            } else {
                debug!("Content preview (first 500 bytes): {}", &content[..500]);
            }
            
            // Send progress update
            if let Some(sender) = &progress_sender {
                sender
                    .send(TranscriptionProgress {
                        status: "Saving transcription file".to_string(),
                        progress: 95.0,
                    })
                    .await
                    .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
            }
            
            // Write content to file
            debug!("Writing content to file: {}", output_path.display());
            let mut output_file = File::create(&output_path).await?;
            output_file.write_all(content.as_bytes()).await?;
            
            // Send completion progress
            if let Some(sender) = &progress_sender {
                sender
                    .send(TranscriptionProgress {
                        status: "Transcription complete".to_string(),
                        progress: 100.0,
                    })
                    .await
                    .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
            }
            
            info!("Transcription completed successfully! Output saved to: {}", output_path.display());
            Ok(output_path)
        },
        Err(err) => {
            error!("Failed to connect to OpenAI API: {}", err);
            
            // Добавляем детальную информацию об ошибке
            let error_message = match err.is_timeout() {
                true => format!("Connection timeout: {}", err),
                false => match err.is_connect() {
                    true => format!("Connection error: {}", err),
                    false => format!("Request error: {}", err),
                }
            };
            
            warn!("Detailed error info: {}", error_message);
            Err(anyhow!("Failed to connect to OpenAI API: {}", error_message))
        }
    }
} 