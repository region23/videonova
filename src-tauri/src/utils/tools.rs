use anyhow::{anyhow, Context, Result};
use log::{debug, info};
use semver::Version;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tokio::sync::mpsc;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use regex::Regex;
use zip;
use walkdir;

// Structure to represent an external tool
#[derive(Debug, Clone)]
pub struct ExternalTool {
    pub name: String,
    pub path: PathBuf,
    pub version: Option<Version>,
    pub min_version: Version,
}

// Global storage for tools
static TOOLS: Lazy<Mutex<Vec<ExternalTool>>> = Lazy::new(|| Mutex::new(Vec::new()));

// Tool download URLs
const YTDLP_DOWNLOAD_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp";
const FFMPEG_DOWNLOAD_URLS: &[(&str, &str)] = &[
    ("windows", "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip"),
    ("macos", "https://evermeet.cx/ffmpeg/getrelease/zip"),
    ("linux", "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz"),
];

/// Initialize external tools
pub async fn init_tools(progress_sender: Option<mpsc::Sender<(String, f32)>>) -> Result<()> {
    info!("Initializing external tools...");
    
    // Send initial progress
    if let Some(sender) = &progress_sender {
        sender.send(("Checking external tools...".to_string(), 0.0)).await?;
    }

    // Check for tools in PATH first
    let ffmpeg_path_result = check_command_in_path("ffmpeg");
    let ytdlp_path_result = check_command_in_path("yt-dlp");

    // Clear existing tools
    let mut tools = TOOLS.lock().unwrap();
    tools.clear();

    // Handle ffmpeg
    match ffmpeg_path_result {
        Ok(path) => {
            info!("Found ffmpeg at {}", path.display());
            if let Ok(version) = check_ffmpeg_version(&path) {
                tools.push(ExternalTool {
                    name: "ffmpeg".to_string(),
                    path,
                    version: Some(version.clone()),
                    min_version: Version::new(4, 0, 0),
                });
                info!("FFmpeg version: {}", version);
            }
        }
        Err(_) => {
            info!("FFmpeg not found in PATH, will attempt to download");
            if let Some(sender) = &progress_sender {
                sender.send(("Downloading FFmpeg...".to_string(), 20.0)).await?;
            }
            match download_ffmpeg().await {
                Ok(path) => {
                    if let Ok(version) = check_ffmpeg_version(&path) {
                        tools.push(ExternalTool {
                            name: "ffmpeg".to_string(),
                            path,
                            version: Some(version),
                            min_version: Version::new(4, 0, 0),
                        });
                    }
                }
                Err(e) => {
                    return Err(anyhow!("Failed to download FFmpeg: {}", e));
                }
            }
        }
    }

    // Handle yt-dlp
    match ytdlp_path_result {
        Ok(path) => {
            info!("Found yt-dlp at {}", path.display());
            if let Ok(version) = check_ytdlp_version(&path) {
                tools.push(ExternalTool {
                    name: "yt-dlp".to_string(),
                    path,
                    version: Some(version.clone()),
                    min_version: Version::new(2023, 11, 16),
                });
                info!("yt-dlp version: {}", version);
            }
        }
        Err(_) => {
            info!("yt-dlp not found in PATH, will attempt to download");
            if let Some(sender) = &progress_sender {
                sender.send(("Downloading yt-dlp...".to_string(), 60.0)).await?;
            }
            match download_ytdlp().await {
                Ok(path) => {
                    if let Ok(version) = check_ytdlp_version(&path) {
                        tools.push(ExternalTool {
                            name: "yt-dlp".to_string(),
                            path,
                            version: Some(version),
                            min_version: Version::new(2023, 11, 16),
                        });
                    }
                }
                Err(e) => {
                    return Err(anyhow!("Failed to download yt-dlp: {}", e));
                }
            }
        }
    }

    if let Some(sender) = &progress_sender {
        sender.send(("Tools initialization completed".to_string(), 100.0)).await?;
    }

    Ok(())
}

/// Check if a command is available in PATH
fn check_command_in_path(command: &str) -> Result<PathBuf> {
    let output = if cfg!(target_os = "windows") {
        Command::new("where")
            .arg(command)
            .output()
    } else {
        Command::new("which")
            .arg(command)
            .output()
    };

    match output {
        Ok(output) if output.status.success() => {
            let path_str = String::from_utf8_lossy(&output.stdout);
            let path = Path::new(path_str.trim()).to_path_buf();
            Ok(path)
        }
        _ => Err(anyhow!("Command {} not found in PATH", command))
    }
}

/// Check ffmpeg version
fn check_ffmpeg_version(path: &Path) -> Result<Version> {
    let output = Command::new(path)
        .args(["-version"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to execute ffmpeg")?;

    if output.status.success() {
        let version_str = String::from_utf8_lossy(&output.stdout);
        let re = Regex::new(r"ffmpeg version (\d+\.\d+(?:\.\d+)?)")?;
        if let Some(caps) = re.captures(&version_str) {
            let version = caps.get(1).map_or("", |m| m.as_str());
            let parts: Vec<&str> = version.split('.').collect();
            let version_str = match parts.len() {
                1 => format!("{}.0.0", parts[0]),
                2 => format!("{}.{}.0", parts[0], parts[1]),
                _ => version.to_string(),
            };
            Ok(Version::parse(&version_str)?)
        } else {
            debug!("Could not parse ffmpeg version, using default");
            Ok(Version::new(4, 0, 0))
        }
    } else {
        Err(anyhow!("Failed to get ffmpeg version"))
    }
}

/// Check yt-dlp version
fn check_ytdlp_version(path: &Path) -> Result<Version> {
    let output = Command::new(path)
        .args(["--version"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to execute yt-dlp")?;

    if output.status.success() {
        let version_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(Version::parse(&version_str)?)
    } else {
        Err(anyhow!("Failed to get yt-dlp version"))
    }
}

/// Download yt-dlp
async fn download_ytdlp() -> Result<PathBuf> {
    let app_dir = tauri::api::path::app_dir(&tauri::Config::default())
        .ok_or_else(|| anyhow!("Failed to get app directory"))?;
    
    let tools_dir = app_dir.join("tools");
    std::fs::create_dir_all(&tools_dir)?;

    let target_path = tools_dir.join(if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    });

    // Download the file
    let response = reqwest::get(YTDLP_DOWNLOAD_URL).await?;
    let content = response.bytes().await?;
    
    // Write to file
    std::fs::write(&target_path, content)?;

    // Make executable on Unix-like systems
    #[cfg(not(target_os = "windows"))]
    std::fs::set_permissions(&target_path, std::fs::Permissions::from_mode(0o755))?;

    Ok(target_path)
}

/// Extract downloaded FFmpeg archive
async fn extract_ffmpeg(archive_path: &Path, target_dir: &Path) -> Result<PathBuf> {
    let extension = archive_path.extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| anyhow!("Invalid archive extension"))?;

    match extension {
        "zip" => {
            let file = std::fs::File::open(archive_path)?;
            let mut archive = zip::ZipArchive::new(file)?;
            
            // Extract all files
            archive.extract(target_dir)?;

            // Find ffmpeg executable in extracted files
            let ffmpeg_name = if cfg!(target_os = "windows") {
                "ffmpeg.exe"
            } else {
                "ffmpeg"
            };

            // Recursively find ffmpeg executable
            let mut ffmpeg_path = None;
            for entry in walkdir::WalkDir::new(target_dir) {
                let entry = entry?;
                if entry.file_name().to_string_lossy() == ffmpeg_name {
                    ffmpeg_path = Some(entry.path().to_path_buf());
                    break;
                }
            }

            let ffmpeg_path = ffmpeg_path.ok_or_else(|| anyhow!("FFmpeg executable not found in archive"))?;

            // Make executable on Unix-like systems
            #[cfg(not(target_os = "windows"))]
            std::fs::set_permissions(&ffmpeg_path, std::fs::Permissions::from_mode(0o755))?;

            Ok(ffmpeg_path)
        }
        "xz" => {
            use std::process::Command;
            
            // For Linux, we use tar command line tool as it's more reliable
            let status = Command::new("tar")
                .args(&["xf", archive_path.to_str().unwrap()])
                .current_dir(target_dir)
                .status()?;

            if !status.success() {
                return Err(anyhow!("Failed to extract tar.xz archive"));
            }

            let ffmpeg_path = target_dir.join("ffmpeg");
            if !ffmpeg_path.exists() {
                return Err(anyhow!("FFmpeg executable not found after extraction"));
            }

            // Make executable
            std::fs::set_permissions(&ffmpeg_path, std::fs::Permissions::from_mode(0o755))?;

            Ok(ffmpeg_path)
        }
        _ => Err(anyhow!("Unsupported archive format: {}", extension))
    }
}

/// Download FFmpeg
async fn download_ffmpeg() -> Result<PathBuf> {
    let app_dir = tauri::api::path::app_dir(&tauri::Config::default())
        .ok_or_else(|| anyhow!("Failed to get app directory"))?;
    
    let tools_dir = app_dir.join("tools");
    std::fs::create_dir_all(&tools_dir)?;

    // Get download URL for current platform
    let (_, url) = FFMPEG_DOWNLOAD_URLS
        .iter()
        .find(|(platform, _)| {
            match *platform {
                "windows" => cfg!(target_os = "windows"),
                "macos" => cfg!(target_os = "macos"),
                "linux" => cfg!(target_os = "linux"),
                _ => false,
            }
        })
        .ok_or_else(|| anyhow!("Unsupported platform"))?;

    // Download the archive
    let response = reqwest::get(*url).await?;
    let content = response.bytes().await?;
    
    // Create a temporary file for the archive
    let temp_dir = tempfile::tempdir()?;
    let archive_path = temp_dir.path().join("ffmpeg_archive");
    std::fs::write(&archive_path, content)?;

    // Extract the archive
    let ffmpeg_path = extract_ffmpeg(&archive_path, &tools_dir).await?;

    // Clean up temporary directory
    drop(temp_dir);

    Ok(ffmpeg_path)
}

/// Get tool path by name
pub fn get_tool_path(name: &str) -> Option<PathBuf> {
    TOOLS.lock()
        .unwrap()
        .iter()
        .find(|tool| tool.name == name)
        .map(|tool| tool.path.clone())
} 