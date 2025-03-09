//! Модуль для кэширования результатов TTS
//! 
//! Этот модуль содержит функции для кэширования результатов генерации речи.

use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use md5;
use crate::error::Result;
use crate::subtitle::parser::Subtitle;
use crate::config::TtsSyncConfig;

/// Структура для управления кэшем
pub struct TtsCache {
    /// Директория для кэша
    cache_dir: PathBuf,
    /// Максимальный размер кэша в байтах
    max_size: Option<u64>,
    /// Карта для отслеживания кэшированных файлов
    cache_map: HashMap<String, String>,
}

impl TtsCache {
    /// Создать новый экземпляр TtsCache
    pub fn new(config: &TtsSyncConfig) -> Result<Self> {
        let cache_dir = if let Some(dir) = &config.cache_dir {
            PathBuf::from(dir)
        } else {
            let temp_dir = std::env::temp_dir();
            temp_dir.join("tts-sync-cache")
        };
        
        // Создаем директорию для кэша, если она не существует
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)?;
        }
        
        Ok(Self {
            cache_dir,
            max_size: config.max_cache_size,
            cache_map: HashMap::new(),
        })
    }
    
    /// Получить путь к кэшированному файлу
    pub fn get_cached_file(&self, subtitle: &Subtitle, voice: &str) -> Option<String> {
        let key = self.generate_cache_key(subtitle, voice);
        self.cache_map.get(&key).cloned()
    }
    
    /// Добавить файл в кэш
    pub fn add_to_cache(&mut self, subtitle: &Subtitle, voice: &str, file_path: &str) -> Result<String> {
        let key = self.generate_cache_key(subtitle, voice);
        let cache_file = self.cache_dir.join(format!("{}.mp3", key));
        
        // Копируем файл в кэш
        fs::copy(file_path, &cache_file)?;
        
        // Добавляем в карту
        let cache_path = cache_file.to_string_lossy().to_string();
        self.cache_map.insert(key, cache_path.clone());
        
        // Проверяем размер кэша
        self.check_cache_size()?;
        
        Ok(cache_path)
    }
    
    /// Очистить кэш
    pub fn clear_cache(&mut self) -> Result<()> {
        for file in fs::read_dir(&self.cache_dir)? {
            let file = file?;
            if file.file_type()?.is_file() {
                fs::remove_file(file.path())?;
            }
        }
        
        self.cache_map.clear();
        
        Ok(())
    }
    
    /// Генерировать ключ для кэша
    fn generate_cache_key(&self, subtitle: &Subtitle, voice: &str) -> String {
        let mut hasher = md5::Context::new();
        hasher.consume(subtitle.text.as_bytes());
        hasher.consume(voice.as_bytes());
        
        format!("{:x}", hasher.compute())
    }
    
    /// Проверить размер кэша
    fn check_cache_size(&self) -> Result<()> {
        if let Some(max_size) = self.max_size {
            let mut total_size = 0;
            let mut files = Vec::new();
            
            // Собираем информацию о файлах
            for entry in fs::read_dir(&self.cache_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    let metadata = entry.metadata()?;
                    total_size += metadata.len();
                    files.push((entry.path(), metadata.modified()?));
                }
            }
            
            // Если размер кэша превышает максимальный, удаляем старые файлы
            if total_size > max_size {
                // Сортируем файлы по времени модификации (от старых к новым)
                files.sort_by(|a, b| a.1.cmp(&b.1));
                
                // Удаляем файлы, пока размер кэша не станет меньше максимального
                for (path, _) in files {
                    if total_size <= max_size {
                        break;
                    }
                    
                    if let Ok(metadata) = fs::metadata(&path) {
                        total_size -= metadata.len();
                        fs::remove_file(path)?;
                    }
                }
            }
        }
        
        Ok(())
    }
}
