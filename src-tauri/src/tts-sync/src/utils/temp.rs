//! Модуль для работы с временными файлами
//! 
//! Этот модуль содержит функции для работы с временными файлами.

use std::path::{Path, PathBuf};
use std::fs;
use tempfile::TempDir;
use crate::error::Result;

/// Менеджер временных файлов
pub struct TempFileManager {
    /// Временная директория
    temp_dir: TempDir,
    /// Список созданных файлов
    files: Vec<PathBuf>,
    /// Нужно ли удалять файлы при завершении
    cleanup: bool,
}

impl TempFileManager {
    /// Создать новый экземпляр TempFileManager
    pub fn new(cleanup: bool) -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        
        Ok(Self {
            temp_dir,
            files: Vec::new(),
            cleanup,
        })
    }
    
    /// Создать временный файл
    pub fn create_temp_file(&mut self, prefix: &str, extension: &str) -> Result<PathBuf> {
        let file_name = format!("{}_{}.{}", prefix, uuid::Uuid::new_v4(), extension);
        let file_path = self.temp_dir.path().join(file_name);
        
        // Создаем пустой файл
        fs::File::create(&file_path)?;
        
        // Добавляем в список
        self.files.push(file_path.clone());
        
        Ok(file_path)
    }
    
    /// Получить путь к временной директории
    pub fn temp_dir_path(&self) -> &Path {
        self.temp_dir.path()
    }
    
    /// Очистить временные файлы
    pub fn cleanup(&mut self) -> Result<()> {
        if self.cleanup {
            for file in &self.files {
                if file.exists() {
                    fs::remove_file(file)?;
                }
            }
            
            self.files.clear();
        }
        
        Ok(())
    }
}

impl Drop for TempFileManager {
    fn drop(&mut self) {
        // Пытаемся очистить файлы при уничтожении объекта
        let _ = self.cleanup();
    }
}
