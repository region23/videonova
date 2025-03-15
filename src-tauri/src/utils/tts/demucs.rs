//! # Demucs Audio Separation
//! 
//! Модуль для удаления вокала из аудио с использованием модели Demucs.
//! Обеспечивает интеграцию с Python-библиотекой Demucs через вызовы внешних процессов.

use std::path::{Path, PathBuf};
use std::process::Command;
use log::{info, warn, error};
use tokio::process::Command as TokioCommand;
use anyhow::Context;

use crate::utils::tts::types::{TtsError, Result};

/// Проверка наличия установленной библиотеки Demucs
pub fn is_demucs_installed() -> bool {
    let output = Command::new("python3")
        .args(&["-c", "import demucs"])
        .output();
    
    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Установка библиотеки Demucs и ее зависимостей через pip
pub async fn install_demucs() -> Result<()> {
    info!("Установка библиотеки Demucs...");
    
    // Сначала устанавливаем torch, если его нет
    let torch_check = Command::new("python3")
        .args(&["-c", "import torch"])
        .status();
    
    if torch_check.is_err() || !torch_check.unwrap().success() {
        info!("Установка PyTorch...");
        let torch_install = TokioCommand::new("pip3")
            .args(&["install", "torch"])
            .status()
            .await
            .map_err(|e| TtsError::Other(anyhow::anyhow!("Ошибка установки PyTorch: {}", e)))?;
            
        if !torch_install.success() {
            return Err(TtsError::Other(anyhow::anyhow!("Не удалось установить PyTorch")));
        }
    }
    
    // Затем устанавливаем demucs
    let demucs_install = TokioCommand::new("pip3")
        .args(&["install", "demucs"])
        .status()
        .await
        .map_err(|e| TtsError::Other(anyhow::anyhow!("Ошибка установки Demucs: {}", e)))?;
        
    if !demucs_install.success() {
        return Err(TtsError::Other(anyhow::anyhow!("Не удалось установить Demucs")));
    }
    
    info!("Demucs успешно установлен");
    Ok(())
}

/// Проверка наличия Demucs и установка при необходимости
pub async fn ensure_demucs_installed() -> Result<()> {
    if !is_demucs_installed() {
        info!("Demucs не установлен, начинаем установку...");
        install_demucs().await?;
    } else {
        info!("Demucs уже установлен");
    }
    Ok(())
}

/// Разделяет аудиофайл на вокал и инструментал с помощью Demucs
/// 
/// # Аргументы
/// 
/// * `input_path` - Путь к входному аудиофайлу
/// * `output_dir` - Директория для сохранения результатов
/// * `model` - Название модели Demucs (по умолчанию "htdemucs")
/// 
/// # Возвращает
/// 
/// Кортеж путей к файлам с инструменталом и вокалом
pub async fn separate_audio<P: AsRef<Path>>(
    input_path: P, 
    output_dir: P,
    model: Option<&str>
) -> Result<(PathBuf, PathBuf)> {
    // Убеждаемся, что Demucs установлен
    ensure_demucs_installed().await?;
    
    let input_path = input_path.as_ref();
    let output_dir = output_dir.as_ref();
    
    // Проверяем существование входного файла
    if !input_path.exists() {
        return Err(TtsError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound, 
            format!("Входной файл не найден: {}", input_path.display())
        )));
    }
    
    // Создаем выходную директорию, если ее нет
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir)
            .map_err(|e| TtsError::IoError(e))?;
    }
    
    let model_name = model.unwrap_or("htdemucs");
    
    info!("Запуск Demucs для разделения аудио: {}", input_path.display());
    info!("Используемая модель: {}", model_name);
    
    let output_status = TokioCommand::new("python3")
        .arg("-m")
        .arg("demucs")
        .arg("--two-stems=vocals")
        .arg(format!("--out={}", output_dir.display()))
        .arg(format!("--model={}", model_name))
        .arg(input_path)
        .status()
        .await
        .map_err(|e| TtsError::Other(anyhow::anyhow!("Ошибка запуска Demucs: {}", e)))?;
    
    if !output_status.success() {
        return Err(TtsError::Other(anyhow::anyhow!("Ошибка при разделении аудио с Demucs")));
    }
    
    // Определяем пути к выходным файлам
    let file_stem = input_path.file_stem().and_then(|s| s.to_str())
        .ok_or_else(|| TtsError::Other(anyhow::anyhow!("Не удалось получить имя файла")))?;
    
    let instrumental_path = output_dir
        .join(model_name)
        .join(file_stem)
        .join("no_vocals.wav");
    
    let vocals_path = output_dir
        .join(model_name)
        .join(file_stem)
        .join("vocals.wav");
    
    // Проверяем, что файлы созданы
    if !instrumental_path.exists() || !vocals_path.exists() {
        return Err(TtsError::Other(anyhow::anyhow!(
            "Результаты обработки не найдены в ожидаемых местах: \
             инструментал: {}, вокал: {}", 
            instrumental_path.display(), vocals_path.display()
        )));
    }
    
    info!("Аудио успешно разделено на инструментал и вокал");
    Ok((instrumental_path, vocals_path))
} 