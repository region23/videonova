use anyhow::{anyhow, Result};
use log::{debug, info, error};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use reqwest;
use std::time::Duration;
use crate::utils::common::{sanitize_filename, check_file_exists_and_valid};

// Progress structure for translation
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranslationProgress {
    pub status: String,
    pub progress: f32,
}

// Structure for VTT segments
#[derive(Debug, Clone)]
struct VttSegment {
    index: usize,
    timestamp: String,
    text: String,
}

// Structure for VTT file
#[derive(Debug)]
struct VttFile {
    header: String,
    segments: Vec<VttSegment>,
}

// Chat message structure for OpenAI API
#[derive(Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

// OpenAI API request
#[derive(Debug, Serialize, Deserialize)]
struct TranslationRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
}

// OpenAI API response
#[derive(Debug, Serialize, Deserialize)]
struct ChatCompletion {
    id: String,
    choices: Vec<Choice>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Choice {
    message: Message,
}

/// Parse VTT file into segments
async fn parse_vtt_file(vtt_path: &Path) -> Result<VttFile> {
    debug!("Parsing VTT file: {}", vtt_path.display());
    
    // Read file content
    let content = fs::read_to_string(vtt_path).await?;
    let lines: Vec<&str> = content.lines().collect();
    
    if lines.is_empty() {
        return Err(anyhow!("VTT file is empty"));
    }
    
    // Extract header (usually "WEBVTT" and metadata)
    let mut header_lines = Vec::new();
    let mut i = 0;
    while i < lines.len() && !lines[i].contains("-->") {
        header_lines.push(lines[i]);
        i += 1;
    }
    
    let header = header_lines.join("\n");
    debug!("VTT header: {}", header);
    
    // Parse segments
    let mut segments = Vec::new();
    let mut current_timestamp = String::new();
    let mut current_text = Vec::new();
    let mut index = 0;
    
    while i < lines.len() {
        let line = lines[i].trim();
        
        // If line contains timestamp
        if line.contains("-->") {
            // If we already have a timestamp and text, add segment
            if !current_timestamp.is_empty() && !current_text.is_empty() {
                segments.push(VttSegment {
                    index,
                    timestamp: current_timestamp,
                    text: current_text.join("\n"),
                });
                index += 1;
                current_text.clear();
            }
            
            current_timestamp = line.to_string();
        } else if !line.is_empty() && !current_timestamp.is_empty() {
            // Add text line to current segment
            current_text.push(line.to_string());
        }
        
        i += 1;
    }
    
    // Add the last segment if any
    if !current_timestamp.is_empty() && !current_text.is_empty() {
        segments.push(VttSegment {
            index,
            timestamp: current_timestamp,
            text: current_text.join("\n"),
        });
    }
    
    debug!("Parsed {} segments from VTT file", segments.len());
    
    Ok(VttFile { header, segments })
}

/// Translate a batch of VTT segments
async fn translate_segments(
    segments: &[VttSegment],
    target_language: &str,
    api_key: &str,
) -> Result<Vec<VttSegment>> {
    debug!("Translating batch of {} segments to {}", segments.len(), target_language);
    
    if segments.is_empty() {
        return Ok(Vec::new());
    }
    
    // Extract text from segments
    let segments_text = segments
        .iter()
        .map(|s| format!("{}. {}", s.index + 1, s.text))
        .collect::<Vec<String>>()
        .join("\n\n");
    
    // Create system message with translation instructions
    let system_message = format!(
        "You are a professional translator. \
        Translate the following subtitles from their original language into {}. \
        Maintain the same format and numbering. \
        Keep the translations natural, accurate, and appropriate for the video context. \
        ONLY include the translated text and numbering in your response.",
        target_language
    );
    
    // Create request to OpenAI API
    let client = reqwest::Client::new();
    let request = TranslationRequest {
        model: "gpt-4o-mini".to_string(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system_message,
            },
            Message {
                role: "user".to_string(),
                content: segments_text,
            },
        ],
        temperature: 0.3,
    };
    
    // Send request to OpenAI API
    debug!("Sending translation request to OpenAI API");
    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .timeout(Duration::from_secs(120))
        .send()
        .await?;
    
    let status = response.status();
    debug!("OpenAI API response status: {}", status);
    
    if !status.is_success() {
        let error_text = response.text().await?;
        error!("OpenAI API error: HTTP {}, body: {}", status, error_text);
        return Err(anyhow!("OpenAI API error: {}", error_text));
    }
    
    // Parse response
    let completion: ChatCompletion = response.json().await?;
    let translated_text = completion.choices[0].message.content.trim();
    debug!("Received translation from OpenAI API");
    
    // Split translated text into segments
    let translated_lines: Vec<&str> = translated_text.lines().collect();
    let mut translated_segments = Vec::new();
    let mut i = 0;
    
    // Create new segments with translated text
    for segment in segments {
        let mut segment_text = Vec::new();
        
        // Find segment start by index
        while i < translated_lines.len() {
            let line = translated_lines[i].trim();
            
            // If line starts with segment index, extract text
            if line.starts_with(&format!("{}.", segment.index + 1)) {
                // Skip the index part
                let text_start = line.find('.').map(|pos| pos + 1).unwrap_or(0);
                let text = line[text_start..].trim().to_string();
                if !text.is_empty() {
                    segment_text.push(text);
                }
                i += 1;
                break;
            }
            i += 1;
        }
        
        // Collect remaining lines for this segment
        while i < translated_lines.len() {
            let line = translated_lines[i].trim();
            
            // If line is empty or starts with next index, break
            if line.is_empty() || (line.contains('.') && line.chars().next().unwrap().is_digit(10)) {
                break;
            }
            
            segment_text.push(line.to_string());
            i += 1;
        }
        
        // Create translated segment
        translated_segments.push(VttSegment {
            index: segment.index,
            timestamp: segment.timestamp.clone(),
            text: segment_text.join("\n"),
        });
    }
    
    debug!("Created {} translated segments", translated_segments.len());
    Ok(translated_segments)
}

/// Translate WebVTT file to target language
/// 
/// # Arguments
/// * `vtt_path` - Path to the source VTT file
/// * `output_dir` - Directory where the translated file will be saved
/// * `target_language_code` - ISO code of the target language (e.g., "es", "fr")
/// * `target_language_name` - Full name of the target language (e.g., "Spanish", "French")
/// * `api_key` - OpenAI API key
/// * `progress_sender` - Optional channel for sending progress updates
pub async fn translate_vtt(
    vtt_path: &Path,
    output_dir: &Path,
    target_language_code: &str,
    target_language_name: &str,
    api_key: &str,
    progress_sender: Option<mpsc::Sender<TranslationProgress>>,
) -> Result<PathBuf> {
    info!("Starting translation of VTT file to {}", target_language_name);
    
    // Validate API key
    if api_key.trim().is_empty() {
        error!("OpenAI API key is empty");
        return Err(anyhow!("OpenAI API key is required for translation"));
    }
    
    // Create output directory if it doesn't exist
    if let Err(e) = fs::create_dir_all(output_dir).await {
        error!("Failed to create output directory: {}", e);
        return Err(anyhow!("Failed to create output directory: {}", e));
    }
    
    // Define output file path
    let file_stem = vtt_path
        .file_stem()
        .ok_or_else(|| anyhow!("Failed to get file stem"))?
        .to_string_lossy();
    
    // Process filename - add language code suffix
    let sanitized_file_stem = sanitize_filename(&format!("{}_{}", file_stem, target_language_code));
    let output_path = output_dir.join(format!("{}.vtt", sanitized_file_stem));
    
    // Check if translation file already exists
    if check_file_exists_and_valid(&output_path).await {
        info!("Found existing translation file");
        return Ok(output_path);
    }
    
    // Send initial progress
    if let Some(sender) = &progress_sender {
        sender
            .send(TranslationProgress {
                status: "Preparing translation".to_string(),
                progress: 0.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }
    
    // Parse VTT file
    let vtt_file = parse_vtt_file(vtt_path).await?;
    
    // Send progress update
    if let Some(sender) = &progress_sender {
        sender
            .send(TranslationProgress {
                status: "Parsed VTT file".to_string(),
                progress: 10.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }
    
    // Split segments into batches (max 30 segments per request to avoid token limits)
    let batch_size = 30;
    let mut translated_segments = Vec::new();
    let total_segments = vtt_file.segments.len();
    
    for (batch_index, batch) in vtt_file.segments.chunks(batch_size).enumerate() {
        let start_segment = batch_index * batch_size;
        let end_segment = std::cmp::min(start_segment + batch_size, total_segments);
        
        // Send progress update
        if let Some(sender) = &progress_sender {
            sender
                .send(TranslationProgress {
                    status: format!("Translating segments {}-{} of {}", start_segment + 1, end_segment, total_segments),
                    progress: 10.0 + (batch_index as f32 / (total_segments / batch_size) as f32) * 80.0,
                })
                .await
                .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
        }
        
        // Translate batch
        let batch_translated = translate_segments(batch, target_language_name, api_key).await?;
        translated_segments.extend(batch_translated);
        
        // Small delay to avoid rate limiting
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    
    // Send progress update
    if let Some(sender) = &progress_sender {
        sender
            .send(TranslationProgress {
                status: "Writing translated file".to_string(),
                progress: 90.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }
    
    // Create translated VTT content
    let mut translated_content = vtt_file.header.clone();
    translated_content.push_str("\n\n");  // Add empty line after header
    
    for segment in &translated_segments {
        translated_content.push_str(&format!("{}\n{}\n\n", segment.timestamp, segment.text));
    }
    
    // Write translated content to file
    let mut file = fs::File::create(&output_path).await?;
    file.write_all(translated_content.as_bytes()).await?;
    
    // Send final progress
    if let Some(sender) = &progress_sender {
        sender
            .send(TranslationProgress {
                status: "Translation completed".to_string(),
                progress: 100.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }
    
    info!("Translation completed: {}", output_path.display());
    Ok(output_path)
} 