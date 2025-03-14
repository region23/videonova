use super::models::{FishSpeechError, FishSpeechResult, SpeechFormat};
use std::path::{Path, PathBuf};
use std::process::Command;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::formats::FormatOptions;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::audio::SampleBuffer;
use std::fs::File;
use std::io::BufReader;
use hound::{WavReader, WavSpec, WavWriter};

/// Convert audio to a different format
pub async fn convert_audio(
    input_path: &Path,
    output_format: SpeechFormat,
) -> FishSpeechResult<PathBuf> {
    // Generate output filename with the correct extension
    let mut output_path = input_path.to_path_buf();
    let stem = output_path.file_stem()
        .ok_or_else(|| FishSpeechError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid input filename"
        )))?
        .to_string_lossy()
        .to_string();
    
    let extension = match output_format {
        SpeechFormat::Wav => "wav",
        SpeechFormat::Mp3 => "mp3",
        SpeechFormat::Ogg => "ogg",
    };
    
    output_path.set_file_name(format!("{}.{}", stem, extension));
    
    // Skip conversion if the input is already in the target format
    if input_path.extension().and_then(|ext| ext.to_str()) == Some(extension) {
        return Ok(input_path.to_path_buf());
    }
    
    // Use ffmpeg for conversion
    let status = Command::new("ffmpeg")
        .args([
            "-y",                    // Overwrite output files
            "-i", input_path.to_str().unwrap(), // Input file
            "-acodec", match output_format {    // Set codec based on format
                SpeechFormat::Wav => "pcm_s16le",
                SpeechFormat::Mp3 => "libmp3lame",
                SpeechFormat::Ogg => "libvorbis",
            },
            output_path.to_str().unwrap(),      // Output file
        ])
        .status()
        .map_err(|e| FishSpeechError::IoError(e))?;
    
    if !status.success() {
        return Err(FishSpeechError::GenerationError(
            "Failed to convert audio format".to_string()
        ));
    }
    
    Ok(output_path)
}

/// Change audio speed using a simple approach (skip samples for speedup, duplicate for slowdown)
pub async fn change_audio_speed(
    input_path: &Path,
    speed_factor: f32,
) -> FishSpeechResult<PathBuf> {
    // Generate output filename
    let mut output_path = input_path.to_path_buf();
    let stem = output_path.file_stem().unwrap().to_string_lossy().to_string();
    let ext = output_path.extension().unwrap().to_string_lossy().to_string();
    output_path.set_file_name(format!("{}_speed_{}.{}", stem, speed_factor, ext));
    
    // Open the input file
    let reader = WavReader::open(input_path)
        .map_err(|e| FishSpeechError::AudioProcessingError(format!("Failed to open WAV file: {}", e)))?;
    
    // Get the spec
    let spec = reader.spec();
    
    // Create a writer with the same spec
    let mut writer = WavWriter::create(&output_path, spec)
        .map_err(|e| FishSpeechError::AudioProcessingError(format!("Failed to create output WAV file: {}", e)))?;
    
    // Read all samples
    let samples: Vec<i32> = reader.into_samples()
        .map(|s| s.unwrap_or(0))
        .collect();
    
    // Process samples based on speed factor
    if speed_factor >= 1.0 {
        // Speedup: skip samples
        let step = speed_factor as usize;
        for i in (0..samples.len()).step_by(step) {
            writer.write_sample(samples[i])
                .map_err(|e| FishSpeechError::AudioProcessingError(format!("Failed to write sample: {}", e)))?;
        }
    } else {
        // Slowdown: duplicate samples
        let repeat = (1.0 / speed_factor) as usize;
        for sample in samples {
            for _ in 0..repeat {
                writer.write_sample(sample)
                    .map_err(|e| FishSpeechError::AudioProcessingError(format!("Failed to write sample: {}", e)))?;
            }
        }
    }
    
    // Finalize the WAV file
    writer.finalize()
        .map_err(|e| FishSpeechError::AudioProcessingError(format!("Failed to finalize WAV file: {}", e)))?;
    
    Ok(output_path)
}

/// Get audio duration in seconds
pub fn get_audio_duration(file_path: &Path) -> FishSpeechResult<f32> {
    // Check if file exists
    if !file_path.exists() {
        return Err(FishSpeechError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "File not found"
        )));
    }
    
    // For WAV files, use hound which is more reliable
    if let Some(ext) = file_path.extension() {
        if ext == "wav" {
            let reader = WavReader::open(file_path)
                .map_err(|e| FishSpeechError::GenerationError(format!("Failed to open WAV file: {}", e)))?;
            
            let spec = reader.spec();
            let num_samples = reader.len();
            let duration = num_samples as f32 / spec.sample_rate as f32;
            
            return Ok(duration);
        }
    }
    
    // For other formats, use a simpler approach
    // This is a fallback that returns a default duration
    // In a real application, you would want to implement proper duration detection
    // for other formats using appropriate libraries
    Ok(5.0) // Default duration of 5 seconds
}

/// Detect silence in audio file
pub fn detect_silence(input_path: &Path, threshold: f32) -> FishSpeechResult<(f32, f32)> {
    // Check if file exists
    if !input_path.exists() {
        return Err(FishSpeechError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "File not found"
        )));
    }
    
    // For WAV files, use hound which is more reliable
    if let Some(ext) = input_path.extension() {
        if ext == "wav" {
            let reader = WavReader::open(input_path)
                .map_err(|e| FishSpeechError::GenerationError(format!("Failed to open WAV file: {}", e)))?;
            
            let spec = reader.spec();
            let samples: Vec<i16> = reader.into_samples::<i16>().filter_map(Result::ok).collect();
            
            if samples.is_empty() {
                return Err(FishSpeechError::GenerationError("Input file has no samples".to_string()));
            }
            
            // Convert threshold to absolute value in i16 range
            let abs_threshold = (threshold * 32768.0) as i16;
            
            // Find start of non-silence
            let mut start_idx = 0;
            while start_idx < samples.len() {
                if samples[start_idx].abs() > abs_threshold {
                    break;
                }
                start_idx += 1;
            }
            
            // Find end of non-silence
            let mut end_idx = samples.len() - 1;
            while end_idx > start_idx {
                if samples[end_idx].abs() > abs_threshold {
                    break;
                }
                end_idx -= 1;
            }
            
            // Convert indices to seconds
            let sample_rate = spec.sample_rate as f32;
            let channels = spec.channels as f32;
            
            let start_time = (start_idx as f32) / (sample_rate * channels);
            let end_time = (end_idx as f32) / (sample_rate * channels);
            
            return Ok((start_time, end_time));
        }
    }
    
    // For other formats, use a simpler approach
    // This is a fallback that returns a default duration
    // In a real application, you would want to implement proper duration detection
    // for other formats using appropriate libraries
    Ok((0.0, 0.0)) // Default duration of 0 seconds
}

/// Trim audio file to specified duration
pub async fn trim_audio(
    input_path: &Path,
    output_path: &Path,
    duration: f32,
) -> FishSpeechResult<PathBuf> {
    // Check if ffmpeg is available
    let ffmpeg_result = Command::new("ffmpeg")
        .arg("-version")
        .output();
    
    if ffmpeg_result.is_ok() {
        // Use ffmpeg for trimming
        let status = Command::new("ffmpeg")
            .args([
                "-i", input_path.to_str().unwrap(),
                "-t", &duration.to_string(),
                "-c", "copy",
                output_path.to_str().unwrap(),
                "-y"
            ])
            .status()
            .map_err(|e| FishSpeechError::IoError(e))?;
        
        if !status.success() {
            return Err(FishSpeechError::GenerationError(
                "Failed to trim audio".to_string()
            ));
        }
    } else {
        // Fallback to manual trimming for WAV files
        if input_path.extension().and_then(|ext| ext.to_str()) == Some("wav") {
            let reader = WavReader::open(input_path)
                .map_err(|e| FishSpeechError::GenerationError(format!("Failed to open WAV file: {}", e)))?;
            
            let spec = reader.spec();
            let sample_rate = spec.sample_rate as f32;
            let num_samples_to_keep = (duration * sample_rate) as u32;
            
            let mut writer = WavWriter::create(output_path, spec)
                .map_err(|e| FishSpeechError::GenerationError(format!("Failed to create output WAV file: {}", e)))?;
            
            for (i, sample) in reader.into_samples::<i32>().enumerate() {
                if i as u32 >= num_samples_to_keep {
                    break;
                }
                
                if let Ok(s) = sample {
                    writer.write_sample(s)
                        .map_err(|e| FishSpeechError::GenerationError(format!("Failed to write sample: {}", e)))?;
                }
            }
            
            writer.finalize()
                .map_err(|e| FishSpeechError::GenerationError(format!("Failed to finalize WAV file: {}", e)))?;
        } else {
            return Err(FishSpeechError::GenerationError(
                "Cannot trim non-WAV files without ffmpeg".to_string()
            ));
        }
    }
    
    Ok(output_path.to_path_buf())
} 