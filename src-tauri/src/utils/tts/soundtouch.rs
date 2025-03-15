//! # SoundTouch Integration
//! 
//! Этот модуль содержит FFI-обертки и вспомогательные функции для интеграции
//! с библиотекой SoundTouch, которая используется для изменения темпа и высоты звука
//! без ухудшения качества.

use std::process::Command;
use std::path::Path;
use log::{info, warn, error};
use anyhow::Context;

use crate::utils::tts::types::{TtsError, Result};

/// Структура для FFI-обертки SoundTouch
#[repr(C)]
pub struct SoundTouch {
    _private: [u8; 0],
}

/// FFI-обёртки для библиотеки SoundTouch.
unsafe extern "C" {
    pub fn soundtouch_createInstance() -> *mut SoundTouch;
    pub fn soundtouch_destroyInstance(instance: *mut SoundTouch);
    pub fn soundtouch_setSampleRate(instance: *mut SoundTouch, srate: u32);
    pub fn soundtouch_setChannels(instance: *mut SoundTouch, numChannels: u32);
    pub fn soundtouch_setTempo(instance: *mut SoundTouch, newTempo: f32);
    pub fn soundtouch_setPitch(instance: *mut SoundTouch, newPitch: f32);
    pub fn soundtouch_putSamples(instance: *mut SoundTouch, samples: *const f32, numSamples: u32);
    pub fn soundtouch_receiveSamples(instance: *mut SoundTouch, outBuffer: *mut f32, maxSamples: u32) -> u32;
}

/// Проверяет, установлена ли библиотека SoundTouch
pub fn is_soundtouch_installed() -> bool {
    #[cfg(target_os = "macos")]
    {
        // На macOS проверяем наличие библиотеки через Homebrew
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
        // На Linux проверяем наличие библиотеки через pkg-config или в стандартных путях
        let pkg_config = Command::new("pkg-config")
            .args(&["--exists", "soundtouch"])
            .status();
            
        match pkg_config {
            Ok(status) => status.success(),
            Err(_) => {
                // Проверим наличие файла библиотеки в стандартных путях
                Path::new("/usr/lib/libSoundTouch.so").exists() || 
                Path::new("/usr/local/lib/libSoundTouch.so").exists()
            },
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        // На Windows проверяем наличие DLL
        Path::new("C:\\Program Files\\SoundTouch\\bin\\SoundTouch.dll").exists() ||
        Path::new("C:\\Program Files (x86)\\SoundTouch\\bin\\SoundTouch.dll").exists()
    }
    
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        false // На других ОС просто возвращаем false
    }
}

/// Устанавливает библиотеку SoundTouch
pub fn install_soundtouch() -> Result<()> {
    info!("Установка библиотеки SoundTouch...");
    
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("brew")
            .args(&["install", "sound-touch"])
            .status()
            .map_err(|e| TtsError::Other(anyhow::anyhow!("Ошибка установки SoundTouch через Homebrew: {}", e)))?;
            
        if !status.success() {
            return Err(TtsError::Other(anyhow::anyhow!("Не удалось установить SoundTouch через Homebrew")));
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        // Пробуем установить через apt для Debian/Ubuntu
        let apt_status = Command::new("apt-get")
            .args(&["install", "-y", "libsoundtouch-dev"])
            .status();
            
        if let Ok(status) = apt_status {
            if status.success() {
                return Ok(());
            }
        }
        
        // Пробуем через pacman для Arch Linux
        let pacman_status = Command::new("pacman")
            .args(&["-S", "--noconfirm", "soundtouch"])
            .status();
            
        if let Ok(status) = pacman_status {
            if status.success() {
                return Ok(());
            }
        }
        
        // Если ни один менеджер пакетов не сработал, возвращаем ошибку
        return Err(TtsError::Other(anyhow::anyhow!("Не удалось установить SoundTouch. Пожалуйста, установите вручную libsoundtouch-dev или аналогичный пакет для вашего дистрибутива")));
    }
    
    #[cfg(target_os = "windows")]
    {
        error!("Автоматическая установка SoundTouch на Windows не поддерживается");
        return Err(TtsError::Other(anyhow::anyhow!("Пожалуйста, скачайте и установите SoundTouch вручную с официального сайта")));
    }
    
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        return Err(TtsError::Other(anyhow::anyhow!("Автоматическая установка SoundTouch не поддерживается для данной ОС")));
    }
    
    info!("SoundTouch успешно установлен");
    Ok(())
}

/// Проверяет, установлен ли SoundTouch, и устанавливает его при необходимости
pub fn ensure_soundtouch_installed() -> Result<()> {
    if !is_soundtouch_installed() {
        info!("SoundTouch не установлен, начинаем установку...");
        install_soundtouch()?;
    } else {
        info!("SoundTouch уже установлен");
    }
    Ok(())
}

/// RAII-обертка для экземпляра SoundTouch
struct SoundTouchInstance(*mut SoundTouch);

impl Drop for SoundTouchInstance {
    fn drop(&mut self) {
        unsafe { soundtouch_destroyInstance(self.0); }
    }
}

/// Обрабатывает аудио через SoundTouch, изменяя темп без изменения высоты звука.
/// 
/// # Аргументы
/// 
/// * `input` - Входные аудио-семплы (моно, f32)
/// * `sample_rate` - Частота дискретизации в Гц
/// * `tempo` - Коэффициент темпа (>1.0 ускоряет, <1.0 замедляет)
/// 
/// # Возвращает
/// 
/// * Обработанные аудио-семплы
pub fn process_with_soundtouch(input: &[f32], sample_rate: u32, tempo: f32) -> Result<Vec<f32>> {
    // Проверка существования библиотеки SoundTouch выполняется на уровне 
    // application, а не здесь, чтобы избежать дублирования

    unsafe {
        let instance = soundtouch_createInstance();
        if instance.is_null() {
            return Err(TtsError::Other(anyhow::anyhow!("Не удалось создать экземпляр SoundTouch")));
        }
        
        // Используем RAII-паттерн для гарантированного уничтожения экземпляра
        let _instance_guard = SoundTouchInstance(instance);
        
        soundtouch_setSampleRate(instance, sample_rate);
        soundtouch_setChannels(instance, 1);
        // Устанавливаем темп (tempo factor) — изменение длительности без изменения pitch.
        soundtouch_setTempo(instance, tempo);
        // Гарантируем, что тон остаётся неизменным.
        soundtouch_setPitch(instance, 1.0);
        // Передаём сэмплы.
        soundtouch_putSamples(instance, input.as_ptr(), input.len() as u32);

        // Считываем обработанные сэмплы.
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