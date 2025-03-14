use anyhow::{Context, Result, anyhow};
use log::{debug, info};
use regex::Regex;
use semver::Version;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tokio::sync::mpsc;
use crate::utils::tools::{ExternalTool, check_command_in_path};

// Tool download URLs
const FFMPEG_DOWNLOAD_URLS: &[(&str, &str)] = &[
    (
        "windows",
        "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip",
    ),
    ("macos", "https://evermeet.cx/ffmpeg/getrelease/zip"),
    (
        "linux",
        "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz",
    ),
];

/// Initialize audio tools (ffmpeg)
pub async fn init_audio_tools(
    initialized_tools: &mut Vec<ExternalTool>,
    progress_sender: &Option<mpsc::Sender<(String, f32)>>,
) -> Result<()> {
    // Check if ffmpeg is already in PATH
    let ffmpeg_path_result = check_command_in_path("ffmpeg");

    // Handle ffmpeg
    let _ffmpeg_path = match ffmpeg_path_result {
        Ok(path) => {
            info!("Found ffmpeg at {}", path.display());
            if let Ok(version) = check_ffmpeg_version(&path) {
                initialized_tools.push(ExternalTool {
                    name: "ffmpeg".to_string(),
                    path: path.clone(),
                    description: "Audio/video processing tool".to_string(),
                    version: Some(version.clone()),
                    min_version: Version::new(4, 0, 0),
                });
                info!("FFmpeg version: {}", version);
                path
            } else {
                // Version check failed, download
                info!("FFmpeg version check failed, will download");
                if let Some(sender) = progress_sender {
                    sender
                        .send(("Downloading FFmpeg...".to_string(), 20.0))
                        .await?;
                }
                let downloaded_path = download_ffmpeg().await?;
                let version = check_ffmpeg_version(&downloaded_path)?;
                initialized_tools.push(ExternalTool {
                    name: "ffmpeg".to_string(),
                    path: downloaded_path.clone(),
                    description: "Audio/video processing tool".to_string(),
                    version: Some(version.clone()),
                    min_version: Version::new(4, 0, 0),
                });
                info!("Downloaded FFmpeg version: {}", version);
                downloaded_path
            }
        }
        Err(_) => {
            info!("FFmpeg not found in PATH, will attempt to download");
            if let Some(sender) = progress_sender {
                sender
                    .send(("Downloading FFmpeg...".to_string(), 20.0))
                    .await?;
            }
            let downloaded_path = download_ffmpeg().await?;
            let version = check_ffmpeg_version(&downloaded_path)?;
            initialized_tools.push(ExternalTool {
                name: "ffmpeg".to_string(),
                path: downloaded_path.clone(),
                description: "Audio/video processing tool".to_string(),
                version: Some(version.clone()),
                min_version: Version::new(4, 0, 0),
            });
            info!("Downloaded FFmpeg version: {}", version);
            downloaded_path
        }
    };

    Ok(())
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

/// Download ffmpeg
async fn download_ffmpeg() -> Result<PathBuf> {
    let temp_dir = std::env::temp_dir();
    let ffmpeg_dir = temp_dir.join("ffmpeg");
    std::fs::create_dir_all(&ffmpeg_dir)?;

    // Get download URL based on OS
    let (os, url) = FFMPEG_DOWNLOAD_URLS
        .iter()
        .find(|(os, _)| os == &std::env::consts::OS)
        .ok_or_else(|| anyhow!("Unsupported operating system"))?;

    // Download archive
    let response = reqwest::get(*url).await?;
    let bytes = response.bytes().await?;
    let archive_path = ffmpeg_dir.join(format!("ffmpeg.{}", if os == &"windows" { "zip" } else { "tar.xz" }));
    std::fs::write(&archive_path, bytes)?;

    // Extract archive
    let ffmpeg_path = extract_ffmpeg(&archive_path, &ffmpeg_dir).await?;

    // Clean up
    std::fs::remove_file(archive_path)?;

    Ok(ffmpeg_path)
}

/// Extract ffmpeg from archive
async fn extract_ffmpeg(archive_path: &Path, target_dir: &Path) -> Result<PathBuf> {
    let os = std::env::consts::OS;
    let ffmpeg_path = if os == "windows" {
        // Extract zip
        let file = std::fs::File::open(archive_path)?;
        let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file))?;
        archive.extract(target_dir)?;
        target_dir.join("ffmpeg-master-latest-win64-gpl/bin/ffmpeg.exe")
    } else {
        // Extract tar.xz
        let status = Command::new("tar")
            .args(["-xf", archive_path.to_str().unwrap(), "-C", target_dir.to_str().unwrap()])
            .status()?;
        if !status.success() {
            return Err(anyhow!("Failed to extract ffmpeg archive"));
        }
        target_dir.join("ffmpeg")
    };

    // Make executable on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&ffmpeg_path, std::fs::Permissions::from_mode(0o755))?;
    }

    Ok(ffmpeg_path)
} 