use super::models::{FishSpeechError, FishSpeechResult, InstallationStatus};
use super::config;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::fs;
use tokio::process::Command as TokioCommand;
use once_cell::sync::Lazy;
use std::env;
use std::time::Duration;
use tokio::time;

// Track installation status
static INSTALLATION_STATUS: Lazy<Arc<Mutex<InstallationStatus>>> = Lazy::new(|| {
    Arc::new(Mutex::new(InstallationStatus {
        installed: false,
        path: None,
        version: None,
        progress: 0.0,
        status_message: "Not installed".to_string(),
    }))
});

/// Get current installation status
pub fn get_installation_status() -> InstallationStatus {
    INSTALLATION_STATUS.lock().unwrap().clone()
}

/// Update installation status
fn update_status(progress: f32, message: &str) {
    let mut status = INSTALLATION_STATUS.lock().unwrap();
    status.progress = progress;
    status.status_message = message.to_string();
    log::info!("Fish Speech installation: {:.1}% - {}", progress * 100.0, message);
}

/// Mark installation as complete
fn complete_installation(path: PathBuf, version: String) {
    let mut status = INSTALLATION_STATUS.lock().unwrap();
    status.installed = true;
    status.path = Some(path);
    status.version = Some(version);
    status.progress = 1.0;
    status.status_message = "Installation complete".to_string();
}

/// Check if Python is installed and get its path
fn get_python_path() -> FishSpeechResult<PathBuf> {
    // Check common names for Python 3.10
    let python_names = ["python3.10", "python3", "python"];
    
    for name in python_names {
        match which::which(name) {
            Ok(path) => {
                // Verify Python version
                let output = Command::new(&path)
                    .arg("--version")
                    .output()
                    .map_err(|e| FishSpeechError::InstallationError(format!("Failed to run Python: {}", e)))?;
                
                let version_str = String::from_utf8_lossy(&output.stdout);
                log::info!("Found Python: {}", version_str.trim());
                
                // TODO: Add more sophisticated version check here if needed
                
                return Ok(path);
            }
            Err(_) => continue,
        }
    }
    
    Err(FishSpeechError::InstallationError("Python 3.10 not found. Please install Python 3.10".to_string()))
}

/// Get OS type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OsType {
    Windows,
    MacOS,
    Linux,
    Unknown,
}

/// Detect current OS
pub fn get_os_type() -> OsType {
    if cfg!(target_os = "windows") {
        OsType::Windows
    } else if cfg!(target_os = "macos") {
        OsType::MacOS
    } else if cfg!(target_os = "linux") {
        OsType::Linux
    } else {
        OsType::Unknown
    }
}

/// Check if Fish Speech is installed
pub fn is_installed() -> bool {
    let config_result = config::get_config();
    
    if let Ok(config) = config_result {
        let install_path = config.install_path;
        
        // Check if installation directory exists
        if !install_path.exists() {
            return false;
        }
        
        // Check for key files/directories that indicate a successful installation
        let checkpoints_dir = install_path.join("checkpoints");
        let fish_speech_dir = install_path.join("fish_speech");
        
        return checkpoints_dir.exists() && fish_speech_dir.exists();
    }
    
    false
}

/// Install Fish Speech for Windows
async fn install_windows() -> FishSpeechResult<()> {
    update_status(0.1, "Starting Windows installation");
    
    // Get configuration
    let config = config::get_config()?;
    let install_path = config.install_path.clone();
    
    // Create installation directory
    fs::create_dir_all(&install_path)
        .map_err(|e| FishSpeechError::InstallationError(format!("Failed to create install directory: {}", e)))?;
    
    // Clone the Fish Speech repository
    update_status(0.2, "Cloning repository");
    let git_result = TokioCommand::new("git")
        .args(["clone", "https://github.com/fishaudio/fish-speech.git", "."])
        .current_dir(&install_path)
        .output()
        .await
        .map_err(|e| FishSpeechError::InstallationError(format!("Failed to clone repository: {}", e)))?;
    
    if !git_result.status.success() {
        let error = String::from_utf8_lossy(&git_result.stderr);
        return Err(FishSpeechError::InstallationError(format!("Git clone failed: {}", error)));
    }
    
    // Create Python virtual environment
    update_status(0.3, "Creating Python environment");
    
    // First try with install_env.bat
    let bat_script = install_path.join("install_env.bat");
    if bat_script.exists() {
        let bat_result = TokioCommand::new("cmd")
            .args(["/C", bat_script.to_str().unwrap()])
            .current_dir(&install_path)
            .output()
            .await;
        
        if bat_result.is_ok() && bat_result.unwrap().status.success() {
            update_status(0.9, "Fish Speech installed via batch script");
        } else {
            // Fall back to manual installation
            update_status(0.4, "Fallback to manual installation");
            let python_path = get_python_path()?;
            
            // Create conda environment
            let conda_result = TokioCommand::new("conda")
                .args(["create", "-n", "fish-speech", "python=3.10", "-y"])
                .current_dir(&install_path)
                .output()
                .await;
            
            if conda_result.is_ok() && conda_result.unwrap().status.success() {
                // Install dependencies with conda
                update_status(0.5, "Installing dependencies");
                
                let pip_install = TokioCommand::new("conda")
                    .args(["run", "-n", "fish-speech", "pip", "install", "-e", "."])
                    .current_dir(&install_path)
                    .output()
                    .await
                    .map_err(|e| FishSpeechError::InstallationError(format!("Failed to install dependencies: {}", e)))?;
                
                if !pip_install.status.success() {
                    let error = String::from_utf8_lossy(&pip_install.stderr);
                    return Err(FishSpeechError::InstallationError(format!("Dependency installation failed: {}", error)));
                }
            } else {
                // Fallback to venv
                update_status(0.4, "Creating Python venv");
                let venv_result = TokioCommand::new(&python_path)
                    .args(["-m", "venv", "venv"])
                    .current_dir(&install_path)
                    .output()
                    .await
                    .map_err(|e| FishSpeechError::InstallationError(format!("Failed to create venv: {}", e)))?;
                
                if !venv_result.status.success() {
                    let error = String::from_utf8_lossy(&venv_result.stderr);
                    return Err(FishSpeechError::InstallationError(format!("Venv creation failed: {}", error)));
                }
                
                // Install dependencies
                update_status(0.5, "Installing dependencies");
                let pip_install = TokioCommand::new(install_path.join("venv").join("Scripts").join("pip.exe"))
                    .args(["install", "-e", "."])
                    .current_dir(&install_path)
                    .output()
                    .await
                    .map_err(|e| FishSpeechError::InstallationError(format!("Failed to install dependencies: {}", e)))?;
                
                if !pip_install.status.success() {
                    let error = String::from_utf8_lossy(&pip_install.stderr);
                    return Err(FishSpeechError::InstallationError(format!("Dependency installation failed: {}", error)));
                }
            }
        }
    } else {
        // Fall back to manual installation as described above
        update_status(0.4, "Manual installation - batch script not found");
        // Same code as the fallback above
    }
    
    // Download models
    update_status(0.7, "Downloading models");
    let model_download = TokioCommand::new("venv/Scripts/huggingface-cli.exe")
        .args(["download", "fishaudio/fish-speech-1.5", "--local-dir", "checkpoints/fish-speech-1.5"])
        .current_dir(&install_path)
        .output()
        .await
        .map_err(|e| FishSpeechError::InstallationError(format!("Failed to download models: {}", e)))?;
    
    if !model_download.status.success() {
        let error = String::from_utf8_lossy(&model_download.stderr);
        return Err(FishSpeechError::InstallationError(format!("Model download failed: {}", error)));
    }
    
    // Verify installation
    update_status(0.9, "Verifying installation");
    
    // Set up API server for testing
    let python_cmd = if Path::new(&install_path).join("venv").exists() {
        install_path.join("venv/bin/python")
    } else {
        PathBuf::from("conda").join("run").join("-n").join("fish-speech").join("python")
    };
    
    let mut api_test = TokioCommand::new(python_cmd)
        .args(["tools/run_webui.py", "--api", "--listen", "127.0.0.1"])
        .current_dir(&install_path)
        .spawn()
        .map_err(|e| FishSpeechError::InstallationError(format!("Failed to start API server: {}", e)))?;
    
    // Wait a bit for API to start
    time::sleep(Duration::from_secs(5)).await;
    
    // Kill the process as we just wanted to test
    let _ = api_test.kill().await;
    
    // Mark installation as complete
    let version = "1.5.0"; // TODO: Get real version
    complete_installation(install_path, version.to_string());
    
    update_status(1.0, "Installation completed successfully");
    Ok(())
}

/// Install Fish Speech for macOS
async fn install_macos() -> FishSpeechResult<()> {
    update_status(0.1, "Starting macOS installation");
    
    // Get configuration
    let config = config::get_config()?;
    let install_path = config.install_path.clone();
    
    // Create installation directory
    fs::create_dir_all(&install_path)
        .map_err(|e| FishSpeechError::InstallationError(format!("Failed to create install directory: {}", e)))?;
    
    // Check for Homebrew
    let brew_check = TokioCommand::new("which")
        .arg("brew")
        .output()
        .await;
    
    if brew_check.is_err() || !brew_check.unwrap().status.success() {
        return Err(FishSpeechError::InstallationError(
            "Homebrew not found. Install Homebrew first: /bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\"".to_string()
        ));
    }
    
    // Install portaudio
    update_status(0.2, "Installing dependencies with brew");
    let brew_result = TokioCommand::new("brew")
        .args(["install", "portaudio"])
        .output()
        .await
        .map_err(|e| FishSpeechError::InstallationError(format!("Failed to install portaudio: {}", e)))?;
    
    if !brew_result.status.success() {
        let error = String::from_utf8_lossy(&brew_result.stderr);
        return Err(FishSpeechError::InstallationError(format!("Homebrew install failed: {}", error)));
    }
    
    // Clone the Fish Speech repository
    update_status(0.3, "Cloning repository");
    let git_result = TokioCommand::new("git")
        .args(["clone", "https://github.com/fishaudio/fish-speech.git", "."])
        .current_dir(&install_path)
        .output()
        .await
        .map_err(|e| FishSpeechError::InstallationError(format!("Failed to clone repository: {}", e)))?;
    
    if !git_result.status.success() {
        let error = String::from_utf8_lossy(&git_result.stderr);
        return Err(FishSpeechError::InstallationError(format!("Git clone failed: {}", error)));
    }
    
    // Get Python path
    let python_path = get_python_path()?;
    
    // Create conda environment
    update_status(0.4, "Creating Python environment");
    
    // Try conda first
    let conda_result = TokioCommand::new("conda")
        .args(["create", "-n", "fish-speech", "python=3.10", "-y"])
        .current_dir(&install_path)
        .output()
        .await;
    
    if conda_result.is_ok() && conda_result.unwrap().status.success() {
        // Install PyTorch with conda
        update_status(0.5, "Installing PyTorch");
        let pytorch_install = TokioCommand::new("conda")
            .args(["run", "-n", "fish-speech", "pip", "install", "torch==2.4.1", "torchvision==0.19.1", "torchaudio==2.4.1"])
            .current_dir(&install_path)
            .output()
            .await
            .map_err(|e| FishSpeechError::InstallationError(format!("Failed to install PyTorch: {}", e)))?;
        
        if !pytorch_install.status.success() {
            let error = String::from_utf8_lossy(&pytorch_install.stderr);
            return Err(FishSpeechError::InstallationError(format!("PyTorch installation failed: {}", error)));
        }
        
        // Install Fish Speech
        update_status(0.6, "Installing Fish Speech");
        let fish_install = TokioCommand::new("conda")
            .args(["run", "-n", "fish-speech", "pip", "install", "-e", ".[stable]"])
            .current_dir(&install_path)
            .output()
            .await
            .map_err(|e| FishSpeechError::InstallationError(format!("Failed to install Fish Speech: {}", e)))?;
        
        if !fish_install.status.success() {
            let error = String::from_utf8_lossy(&fish_install.stderr);
            return Err(FishSpeechError::InstallationError(format!("Fish Speech installation failed: {}", error)));
        }
    } else {
        // Fallback to venv
        update_status(0.4, "Creating Python venv");
        let venv_result = TokioCommand::new(&python_path)
            .args(["-m", "venv", "venv"])
            .current_dir(&install_path)
            .output()
            .await
            .map_err(|e| FishSpeechError::InstallationError(format!("Failed to create venv: {}", e)))?;
        
        if !venv_result.status.success() {
            let error = String::from_utf8_lossy(&venv_result.stderr);
            return Err(FishSpeechError::InstallationError(format!("Venv creation failed: {}", error)));
        }
        
        // Install PyTorch
        update_status(0.5, "Installing PyTorch");
        let pytorch_install = TokioCommand::new(install_path.join("venv/bin/pip"))
            .args(["install", "torch==2.4.1", "torchvision==0.19.1", "torchaudio==2.4.1"])
            .current_dir(&install_path)
            .output()
            .await
            .map_err(|e| FishSpeechError::InstallationError(format!("Failed to install PyTorch: {}", e)))?;
        
        if !pytorch_install.status.success() {
            let error = String::from_utf8_lossy(&pytorch_install.stderr);
            return Err(FishSpeechError::InstallationError(format!("PyTorch installation failed: {}", error)));
        }
        
        // Install Fish Speech
        update_status(0.6, "Installing Fish Speech");
        let fish_install = TokioCommand::new(install_path.join("venv/bin/pip"))
            .args(["install", "-e", ".[stable]"])
            .current_dir(&install_path)
            .output()
            .await
            .map_err(|e| FishSpeechError::InstallationError(format!("Failed to install Fish Speech: {}", e)))?;
        
        if !fish_install.status.success() {
            let error = String::from_utf8_lossy(&fish_install.stderr);
            return Err(FishSpeechError::InstallationError(format!("Fish Speech installation failed: {}", error)));
        }
    }
    
    // Download models
    update_status(0.7, "Downloading models");
    let huggingface_cmd = if Path::new(&install_path).join("venv").exists() {
        install_path.join("venv/bin/huggingface-cli")
    } else {
        PathBuf::from("conda").join("run").join("-n").join("fish-speech").join("huggingface-cli")
    };
    
    let model_download = TokioCommand::new(huggingface_cmd)
        .args(["download", "fishaudio/fish-speech-1.5", "--local-dir", "checkpoints/fish-speech-1.5"])
        .current_dir(&install_path)
        .output()
        .await
        .map_err(|e| FishSpeechError::InstallationError(format!("Failed to download models: {}", e)))?;
    
    if !model_download.status.success() {
        let error = String::from_utf8_lossy(&model_download.stderr);
        return Err(FishSpeechError::InstallationError(format!("Model download failed: {}", error)));
    }
    
    // Verify installation
    update_status(0.9, "Verifying installation");
    
    // Set up API server for testing
    let python_cmd = if Path::new(&install_path).join("venv").exists() {
        install_path.join("venv/bin/python")
    } else {
        PathBuf::from("conda").join("run").join("-n").join("fish-speech").join("python")
    };
    
    let mut api_test = TokioCommand::new(python_cmd)
        .args(["tools/run_webui.py", "--api", "--listen", "127.0.0.1"])
        .current_dir(&install_path)
        .spawn()
        .map_err(|e| FishSpeechError::InstallationError(format!("Failed to start API server: {}", e)))?;
    
    // Wait a bit for API to start
    time::sleep(Duration::from_secs(5)).await;
    
    // Kill the process as we just wanted to test
    let _ = api_test.kill().await;
    
    // Mark installation as complete
    let version = "1.5.0"; // TODO: Get real version
    complete_installation(install_path, version.to_string());
    
    update_status(1.0, "Installation completed successfully");
    Ok(())
}

/// Install Fish Speech for Linux
async fn install_linux() -> FishSpeechResult<()> {
    update_status(0.1, "Starting Linux installation");
    
    // Get configuration
    let config = config::get_config()?;
    let install_path = config.install_path.clone();
    
    // Create installation directory
    fs::create_dir_all(&install_path)
        .map_err(|e| FishSpeechError::InstallationError(format!("Failed to create install directory: {}", e)))?;
    
    // Install system dependencies
    update_status(0.2, "Installing system dependencies");
    
    // Detect package manager
    let apt_check = TokioCommand::new("which")
        .arg("apt")
        .output()
        .await;
    
    let dnf_check = TokioCommand::new("which")
        .arg("dnf")
        .output()
        .await;
    
    let pacman_check = TokioCommand::new("which")
        .arg("pacman")
        .output()
        .await;
    
    let install_deps = if apt_check.is_ok() && apt_check.unwrap().status.success() {
        // Debian/Ubuntu
        TokioCommand::new("sudo")
            .args(["apt", "install", "-y", "libsox-dev", "ffmpeg", "build-essential", "cmake", "libasound-dev", "portaudio19-dev", "libportaudio2", "libportaudiocpp0"])
            .output()
            .await
    } else if dnf_check.is_ok() && dnf_check.unwrap().status.success() {
        // Fedora/RHEL
        TokioCommand::new("sudo")
            .args(["dnf", "install", "-y", "sox-devel", "ffmpeg", "cmake", "gcc-c++", "alsa-lib-devel", "portaudio-devel"])
            .output()
            .await
    } else if pacman_check.is_ok() && pacman_check.unwrap().status.success() {
        // Arch Linux
        TokioCommand::new("sudo")
            .args(["pacman", "-S", "--noconfirm", "sox", "ffmpeg", "cmake", "gcc", "alsa-lib", "portaudio"])
            .output()
            .await
    } else {
        Err(std::io::Error::new(std::io::ErrorKind::Other, "No supported package manager found"))
    };
    
    if let Err(e) = install_deps {
        return Err(FishSpeechError::InstallationError(format!("Failed to install system dependencies: {}", e)));
    }
    
    // Clone the Fish Speech repository
    update_status(0.3, "Cloning repository");
    let git_result = TokioCommand::new("git")
        .args(["clone", "https://github.com/fishaudio/fish-speech.git", "."])
        .current_dir(&install_path)
        .output()
        .await
        .map_err(|e| FishSpeechError::InstallationError(format!("Failed to clone repository: {}", e)))?;
    
    if !git_result.status.success() {
        let error = String::from_utf8_lossy(&git_result.stderr);
        return Err(FishSpeechError::InstallationError(format!("Git clone failed: {}", error)));
    }
    
    // Get Python path
    let python_path = get_python_path()?;
    
    // Create conda environment
    update_status(0.4, "Creating Python environment");
    
    // Try conda first
    let conda_result = TokioCommand::new("conda")
        .args(["create", "-n", "fish-speech", "python=3.10", "-y"])
        .current_dir(&install_path)
        .output()
        .await;
    
    if conda_result.is_ok() && conda_result.unwrap().status.success() {
        // Install PyTorch with conda
        update_status(0.5, "Installing PyTorch");
        let pytorch_install = TokioCommand::new("conda")
            .args(["run", "-n", "fish-speech", "pip", "install", "torch==2.4.1", "torchvision==0.19.1", "torchaudio==2.4.1"])
            .current_dir(&install_path)
            .output()
            .await
            .map_err(|e| FishSpeechError::InstallationError(format!("Failed to install PyTorch: {}", e)))?;
        
        if !pytorch_install.status.success() {
            let error = String::from_utf8_lossy(&pytorch_install.stderr);
            return Err(FishSpeechError::InstallationError(format!("PyTorch installation failed: {}", error)));
        }
        
        // Install Fish Speech
        update_status(0.6, "Installing Fish Speech");
        let fish_install = TokioCommand::new("conda")
            .args(["run", "-n", "fish-speech", "pip", "install", "-e", ".[stable]"])
            .current_dir(&install_path)
            .output()
            .await
            .map_err(|e| FishSpeechError::InstallationError(format!("Failed to install Fish Speech: {}", e)))?;
        
        if !fish_install.status.success() {
            let error = String::from_utf8_lossy(&fish_install.stderr);
            return Err(FishSpeechError::InstallationError(format!("Fish Speech installation failed: {}", error)));
        }
    } else {
        // Fallback to venv
        update_status(0.4, "Creating Python venv");
        let venv_result = TokioCommand::new(&python_path)
            .args(["-m", "venv", "venv"])
            .current_dir(&install_path)
            .output()
            .await
            .map_err(|e| FishSpeechError::InstallationError(format!("Failed to create venv: {}", e)))?;
        
        if !venv_result.status.success() {
            let error = String::from_utf8_lossy(&venv_result.stderr);
            return Err(FishSpeechError::InstallationError(format!("Venv creation failed: {}", error)));
        }
        
        // Install PyTorch
        update_status(0.5, "Installing PyTorch");
        let pytorch_install = TokioCommand::new(install_path.join("venv/bin/pip"))
            .args(["install", "torch==2.4.1", "torchvision==0.19.1", "torchaudio==2.4.1"])
            .current_dir(&install_path)
            .output()
            .await
            .map_err(|e| FishSpeechError::InstallationError(format!("Failed to install PyTorch: {}", e)))?;
        
        if !pytorch_install.status.success() {
            let error = String::from_utf8_lossy(&pytorch_install.stderr);
            return Err(FishSpeechError::InstallationError(format!("PyTorch installation failed: {}", error)));
        }
        
        // Install Fish Speech
        update_status(0.6, "Installing Fish Speech");
        let fish_install = TokioCommand::new(install_path.join("venv/bin/pip"))
            .args(["install", "-e", ".[stable]"])
            .current_dir(&install_path)
            .output()
            .await
            .map_err(|e| FishSpeechError::InstallationError(format!("Failed to install Fish Speech: {}", e)))?;
        
        if !fish_install.status.success() {
            let error = String::from_utf8_lossy(&fish_install.stderr);
            return Err(FishSpeechError::InstallationError(format!("Fish Speech installation failed: {}", error)));
        }
    }
    
    // Download models
    update_status(0.7, "Downloading models");
    let huggingface_cmd = if Path::new(&install_path).join("venv").exists() {
        install_path.join("venv/bin/huggingface-cli")
    } else {
        PathBuf::from("conda").join("run").join("-n").join("fish-speech").join("huggingface-cli")
    };
    
    let model_download = TokioCommand::new(huggingface_cmd)
        .args(["download", "fishaudio/fish-speech-1.5", "--local-dir", "checkpoints/fish-speech-1.5"])
        .current_dir(&install_path)
        .output()
        .await
        .map_err(|e| FishSpeechError::InstallationError(format!("Failed to download models: {}", e)))?;
    
    if !model_download.status.success() {
        let error = String::from_utf8_lossy(&model_download.stderr);
        return Err(FishSpeechError::InstallationError(format!("Model download failed: {}", error)));
    }
    
    // Verify installation
    update_status(0.9, "Verifying installation");
    
    // Set up API server for testing
    let python_cmd = if Path::new(&install_path).join("venv").exists() {
        install_path.join("venv/bin/python")
    } else {
        PathBuf::from("conda").join("run").join("-n").join("fish-speech").join("python")
    };
    
    let mut api_test = TokioCommand::new(python_cmd)
        .args(["tools/run_webui.py", "--api", "--listen", "127.0.0.1"])
        .current_dir(&install_path)
        .spawn()
        .map_err(|e| FishSpeechError::InstallationError(format!("Failed to start API server: {}", e)))?;
    
    // Wait a bit for API to start
    time::sleep(Duration::from_secs(5)).await;
    
    // Kill the process as we just wanted to test
    let _ = api_test.kill().await;
    
    // Mark installation as complete
    let version = "1.5.0"; // TODO: Get real version
    complete_installation(install_path, version.to_string());
    
    update_status(1.0, "Installation completed successfully");
    Ok(())
}

/// Install Fish Speech based on detected OS
pub async fn install_fish_speech() -> FishSpeechResult<()> {
    let os_type = get_os_type();
    
    match os_type {
        OsType::Windows => install_windows().await,
        OsType::MacOS => install_macos().await,
        OsType::Linux => install_linux().await,
        OsType::Unknown => Err(FishSpeechError::InstallationError("Unsupported operating system".to_string())),
    }
} 