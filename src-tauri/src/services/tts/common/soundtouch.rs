use std::path::Path;
use std::process::Command;
use log::{info, warn, error};
use crate::errors::{AppError, AppResult};

/// Структура для FFI-обертки SoundTouch
#[repr(C)]
pub struct SoundTouch {
    _private: [u8; 0],
}

/// FFI-обёртки для библиотеки SoundTouch
unsafe extern "C" {
    fn soundtouch_createInstance() -> *mut SoundTouch;
    fn soundtouch_destroyInstance(instance: *mut SoundTouch);
    fn soundtouch_setSampleRate(instance: *mut SoundTouch, srate: u32);
    fn soundtouch_setChannels(instance: *mut SoundTouch, numChannels: u32);
    fn soundtouch_setTempo(instance: *mut SoundTouch, newTempo: f32);
    fn soundtouch_setPitch(instance: *mut SoundTouch, newPitch: f32);
    fn soundtouch_putSamples(instance: *mut SoundTouch, samples: *const f32, numSamples: u32);
    fn soundtouch_receiveSamples(instance: *mut SoundTouch, outBuffer: *mut f32, maxSamples: u32) -> u32;
}

/// Проверяет, установлена ли библиотека SoundTouch
pub fn is_soundtouch_installed() -> bool {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("brew")
            .args(&["list", "sound-touch"])
            .output();
        
        match output {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        let pkg_config = Command::new("pkg-config")
            .args(&["--exists", "soundtouch"])
            .status();
            
        match pkg_config {
            Ok(status) => status.success(),
            Err(_) => {
                Path::new("/usr/lib/libSoundTouch.so").exists() || 
                Path::new("/usr/local/lib/libSoundTouch.so").exists()
            },
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        Path::new("C:\\Program Files\\SoundTouch\\bin\\SoundTouch.dll").exists() ||
        Path::new("C:\\Program Files (x86)\\SoundTouch\\bin\\SoundTouch.dll").exists()
    }
    
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        false
    }
}

/// Устанавливает библиотеку SoundTouch
pub fn install_soundtouch() -> AppResult<()> {
    info!("Установка библиотеки SoundTouch...");
    
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("brew")
            .args(&["install", "sound-touch"])
            .status()
            .map_err(|e| AppError::InstallationError(format!("Ошибка установки SoundTouch через Homebrew: {}", e)))?;
            
        if !status.success() {
            return Err(AppError::InstallationError("Не удалось установить SoundTouch через Homebrew".to_string()));
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        let apt_status = Command::new("apt-get")
            .args(&["install", "-y", "libsoundtouch-dev"])
            .status();
            
        if let Ok(status) = apt_status {
            if status.success() {
                return Ok(());
            }
        }
        
        let pacman_status = Command::new("pacman")
            .args(&["-S", "--noconfirm", "soundtouch"])
            .status();
            
        if let Ok(status) = pacman_status {
            if status.success() {
                return Ok(());
            }
        }
        
        return Err(AppError::InstallationError("Не удалось установить SoundTouch. Пожалуйста, установите вручную libsoundtouch-dev или аналогичный пакет для вашего дистрибутива".to_string()));
    }
    
    #[cfg(target_os = "windows")]
    {
        error!("Автоматическая установка SoundTouch на Windows не поддерживается");
        return Err(AppError::InstallationError("Пожалуйста, скачайте и установите SoundTouch вручную с официального сайта".to_string()));
    }
    
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        return Err(AppError::InstallationError("Автоматическая установка SoundTouch не поддерживается для данной ОС".to_string()));
    }
    
    info!("SoundTouch успешно установлен");
    Ok(())
}

/// Проверяет, установлен ли SoundTouch, и устанавливает его при необходимости
pub fn ensure_soundtouch_installed() -> AppResult<()> {
    if !is_soundtouch_installed() {
        info!("SoundTouch не установлен, начинаем установку...");
        install_soundtouch()?;
    } else {
        info!("SoundTouch уже установлен");
    }
    Ok(())
}

/// Обёртка для обработки аудио через SoundTouch с сохранением pitch
pub fn process_with_soundtouch(input: &[f32], sample_rate: u32, tempo: f32) -> AppResult<Vec<f32>> {
    unsafe {
        let instance = soundtouch_createInstance();
        if instance.is_null() {
            return Err(AppError::AudioProcessingError("Не удалось создать экземпляр SoundTouch".to_string()));
        }
        
        struct SoundTouchInstance(*mut SoundTouch);
        impl Drop for SoundTouchInstance {
            fn drop(&mut self) {
                unsafe { soundtouch_destroyInstance(self.0); }
            }
        }
        let _instance_guard = SoundTouchInstance(instance);
        
        soundtouch_setSampleRate(instance, sample_rate);
        soundtouch_setChannels(instance, 1);
        soundtouch_setTempo(instance, tempo);
        soundtouch_setPitch(instance, 1.0);
        soundtouch_putSamples(instance, input.as_ptr(), input.len() as u32);

        let mut output = Vec::new();
        let mut buffer = vec![0f32; 1024];
        loop {
            let received = soundtouch_receiveSamples(instance, buffer.as_mut_ptr(), buffer.len() as u32);
            if received == 0 {
                break;
            }
            output.extend_from_slice(&buffer[..received as usize]);
        }
        
        Ok(output)
    }
}

pub struct SoundTouchProcessor {
    instance: *mut SoundTouch,
}

impl SoundTouchProcessor {
    pub fn new() -> Self {
        let instance = unsafe { soundtouch_createInstance() };
        Self { instance }
    }
    
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        unsafe { soundtouch_setSampleRate(self.instance, sample_rate); }
    }
    
    pub fn set_channels(&mut self, channels: u32) {
        unsafe { soundtouch_setChannels(self.instance, channels); }
    }
    
    pub fn set_tempo(&mut self, tempo: f32) {
        unsafe { soundtouch_setTempo(self.instance, tempo); }
    }
    
    pub fn set_pitch(&mut self, pitch: f32) {
        unsafe { soundtouch_setPitch(self.instance, pitch); }
    }
    
    pub fn put_samples(&mut self, samples: &[f32]) {
        unsafe {
            soundtouch_putSamples(
                self.instance,
                samples.as_ptr(),
                samples.len() as u32,
            );
        }
    }
    
    pub fn receive_samples(&mut self, out_buffer: &mut [f32]) -> u32 {
        unsafe {
            soundtouch_receiveSamples(
                self.instance,
                out_buffer.as_mut_ptr(),
                out_buffer.len() as u32,
            )
        }
    }
}

impl Drop for SoundTouchProcessor {
    fn drop(&mut self) {
        unsafe { soundtouch_destroyInstance(self.instance); }
    }
}

unsafe impl Send for SoundTouchProcessor {} 