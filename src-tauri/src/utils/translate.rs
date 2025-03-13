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

// Parse VTT file into segments
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

// Translate a batch of VTT segments
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

// Translate VTT file
pub async fn translate_vtt(
    vtt_path: &Path,
    output_dir: &Path,
    target_language_code: &str,
    target_language_name: &str,
    api_key: &str,
    progress_sender: Option<mpsc::Sender<TranslationProgress>>,
) -> Result<PathBuf> {
    info!("Starting VTT translation to {}", target_language_name);
    
    // Create output directory if it doesn't exist
    fs::create_dir_all(output_dir).await?;
    
    // Create output file path with language suffix
    let file_stem = vtt_path
        .file_stem()
        .ok_or_else(|| anyhow!("Failed to get file stem"))?
        .to_string_lossy();
    
    let sanitized_file_stem = sanitize_filename(&file_stem);
    let output_path = output_dir.join(format!("{}_{}.vtt", sanitized_file_stem, target_language_code));
    debug!("Output will be saved to: {}", output_path.display());

    // Check if translation file already exists
    if check_file_exists_and_valid(&output_path).await {
        info!("Found existing translation file, skipping translation");
        return Ok(output_path);
    }
    
    // Parse VTT file
    if let Some(sender) = &progress_sender {
        sender
            .send(TranslationProgress {
                status: "Parsing VTT file".to_string(),
                progress: 0.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }
    
    let vtt_file = parse_vtt_file(vtt_path).await?;
    debug!("Successfully parsed VTT file with {} segments", vtt_file.segments.len());
    
    if vtt_file.segments.is_empty() {
        return Err(anyhow!("No segments found in VTT file"));
    }
    
    // Process in batches of 10 segments
    const BATCH_SIZE: usize = 10;
    let total_segments = vtt_file.segments.len();
    let batch_count = (total_segments + BATCH_SIZE - 1) / BATCH_SIZE;
    
    info!("Starting translation in {} batches", batch_count);
    
    let mut translated_segments = Vec::new();
    
    for (batch_index, chunk) in vtt_file.segments.chunks(BATCH_SIZE).enumerate() {
        if let Some(sender) = &progress_sender {
            let progress = (batch_index as f32 / batch_count as f32) * 100.0;
            sender
                .send(TranslationProgress {
                    status: format!("Translating segments ({}/{})", batch_index + 1, batch_count),
                    progress,
                })
                .await
                .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
        }
        
        debug!("Translating batch {}/{}", batch_index + 1, batch_count);
        let batch_translated = translate_segments(chunk, target_language_name, api_key).await?;
        translated_segments.extend(batch_translated);
        
        // Small delay to avoid API rate limits
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    // Write translated VTT to file
    if let Some(sender) = &progress_sender {
        sender
            .send(TranslationProgress {
                status: "Saving translated VTT file".to_string(),
                progress: 95.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }
    
    let mut output_file = fs::File::create(&output_path).await?;
    
    // Write header
    output_file.write_all(vtt_file.header.as_bytes()).await?;
    output_file.write_all(b"\n\n").await?;
    
    // Write translated segments
    for segment in &translated_segments {
        output_file.write_all(segment.timestamp.as_bytes()).await?;
        output_file.write_all(b"\n").await?;
        output_file.write_all(segment.text.as_bytes()).await?;
        output_file.write_all(b"\n\n").await?;
    }
    
    info!("Translation complete. Saved to: {}", output_path.display());
    
    // Final progress update
    if let Some(sender) = &progress_sender {
        sender
            .send(TranslationProgress {
                status: "Translation complete".to_string(),
                progress: 100.0,
            })
            .await
            .map_err(|e| anyhow!("Failed to send progress: {}", e))?;
    }
    
    Ok(output_path)
} 