use anyhow::{Context, Result, anyhow};
use log::{debug, info};
use regex::Regex;
use semver::Version;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tokio::sync::mpsc;
use crate::utils::tools::{ExternalTool, check_command_in_path};

// Tool download URLs
const YTDLP_DOWNLOAD_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp";

/// Initialize video tools (yt-dlp)
pub async fn init_video_tools(
    initialized_tools: &mut Vec<ExternalTool>,
    progress_sender: &Option<mpsc::Sender<(String, f32)>>,
) -> Result<()> {
    // Check if yt-dlp is already in PATH
    let ytdlp_path_result = check_command_in_path("yt-dlp");

    // Handle yt-dlp
    let _ytdlp_path = match ytdlp_path_result {
        Ok(path) => {
            info!("Found yt-dlp at {}", path.display());
            if let Ok(version) = check_ytdlp_version(&path) {
                initialized_tools.push(ExternalTool {
                    name: "yt-dlp".to_string(),
                    path: path.clone(),
                    description: "YouTube video downloader".to_string(),
                    version: Some(version.clone()),
                    min_version: Version::new(23, 11, 0),
                });
                info!("yt-dlp version: {}", version);
                path
            } else {
                // Version check failed, download
                info!("yt-dlp version check failed, will download");
                if let Some(sender) = progress_sender {
                    sender
                        .send(("Downloading yt-dlp...".to_string(), 60.0))
                        .await?;
                }
                let downloaded_path = download_ytdlp().await?;
                let version = check_ytdlp_version(&downloaded_path)?;
                initialized_tools.push(ExternalTool {
                    name: "yt-dlp".to_string(),
                    path: downloaded_path.clone(),
                    description: "YouTube video downloader".to_string(),
                    version: Some(version.clone()),
                    min_version: Version::new(23, 11, 0),
                });
                info!("Downloaded yt-dlp version: {}", version);
                downloaded_path
            }
        }
        Err(_) => {
            info!("yt-dlp not found in PATH, will attempt to download");
            if let Some(sender) = progress_sender {
                sender
                    .send(("Downloading yt-dlp...".to_string(), 60.0))
                    .await?;
            }
            let downloaded_path = download_ytdlp().await?;
            let version = check_ytdlp_version(&downloaded_path)?;
            initialized_tools.push(ExternalTool {
                name: "yt-dlp".to_string(),
                path: downloaded_path.clone(),
                description: "YouTube video downloader".to_string(),
                version: Some(version.clone()),
                min_version: Version::new(23, 11, 0),
            });
            info!("Downloaded yt-dlp version: {}", version);
            downloaded_path
        }
    };

    Ok(())
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
        debug!("Raw yt-dlp version: {}", version_str);

        // Use regex to extract version numbers
        let re = Regex::new(r"(\d+)\.(\d+)\.(\d+)")?;
        if let Some(caps) = re.captures(&version_str) {
            let major = caps.get(1).map_or("0", |m| m.as_str()).parse::<u64>()?;
            let minor = caps.get(2).map_or("0", |m| m.as_str()).parse::<u64>()?;
            let patch = caps.get(3).map_or("0", |m| m.as_str()).parse::<u64>()?;

            debug!("Parsed yt-dlp version: {}.{}.{}", major, minor, patch);
            return Ok(Version::new(major, minor, patch));
        }
    }
    Err(anyhow!("Failed to get yt-dlp version"))
}

/// Download yt-dlp
async fn download_ytdlp() -> Result<PathBuf> {
    let temp_dir = std::env::temp_dir();
    let ytdlp_path = temp_dir.join("yt-dlp");

    // Download yt-dlp
    let response = reqwest::get(YTDLP_DOWNLOAD_URL).await?;
    let bytes = response.bytes().await?;
    std::fs::write(&ytdlp_path, bytes)?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&ytdlp_path, std::fs::Permissions::from_mode(0o755))?;
    }

    Ok(ytdlp_path)
} 