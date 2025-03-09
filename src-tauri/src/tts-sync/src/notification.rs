//! Модуль для реализации системы уведомлений
//! 
//! Этот модуль предоставляет конкретные реализации наблюдателей для
//! системы прогресса и уведомлений библиотеки tts-sync.

use std::sync::{Arc, Mutex};
use std::fmt;
use std::io::Write;
use tokio::sync::mpsc;
use crate::progress::{ProgressObserver, ProgressInfo};

/// Наблюдатель, выводящий информацию о прогрессе в консоль
pub struct ConsoleProgressObserver {
    /// Префикс для вывода (опционально)
    prefix: Option<String>,
}

impl ConsoleProgressObserver {
    /// Создать новый экземпляр ConsoleProgressObserver
    pub fn new() -> Self {
        Self { prefix: None }
    }
    
    /// Создать новый экземпляр ConsoleProgressObserver с префиксом
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self { prefix: Some(prefix.into()) }
    }
}

impl Default for ConsoleProgressObserver {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressObserver for ConsoleProgressObserver {
    fn on_progress_update(&self, progress: ProgressInfo) {
        let prefix = self.prefix.as_deref().unwrap_or("");
        let details = progress.details.as_deref().unwrap_or("");
        
        println!(
            "{}[Прогресс] Шаг: {}, Прогресс шага: {:.1}%, Общий прогресс: {:.1}%{}",
            prefix,
            progress.step,
            progress.step_progress,
            progress.total_progress,
            if details.is_empty() { "".to_string() } else { format!(", Детали: {}", details) }
        );
    }
}

/// Наблюдатель, сохраняющий информацию о прогрессе в памяти
pub struct MemoryProgressObserver {
    /// История обновлений прогресса
    history: Arc<Mutex<Vec<ProgressInfo>>>,
}

impl MemoryProgressObserver {
    /// Создать новый экземпляр MemoryProgressObserver
    pub fn new() -> Self {
        Self {
            history: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// Получить историю обновлений прогресса
    pub fn history(&self) -> Vec<ProgressInfo> {
        let history = self.history.lock().unwrap();
        history.clone()
    }
    
    /// Очистить историю обновлений прогресса
    pub fn clear_history(&self) {
        let mut history = self.history.lock().unwrap();
        history.clear();
    }
}

impl Default for MemoryProgressObserver {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressObserver for MemoryProgressObserver {
    fn on_progress_update(&self, progress: ProgressInfo) {
        let mut history = self.history.lock().unwrap();
        history.push(progress);
    }
}

/// Наблюдатель, записывающий информацию о прогрессе в файл
pub struct FileProgressObserver {
    /// Путь к файлу
    file_path: String,
}

impl FileProgressObserver {
    /// Создать новый экземпляр FileProgressObserver
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
        }
    }
}

impl ProgressObserver for FileProgressObserver {
    fn on_progress_update(&self, progress: ProgressInfo) {
        let details = progress.details.as_deref().unwrap_or("");
        let log_entry = format!(
            "[{}] Шаг: {}, Прогресс шага: {:.1}%, Общий прогресс: {:.1}%{}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            progress.step,
            progress.step_progress,
            progress.total_progress,
            if details.is_empty() { "".to_string() } else { format!(", Детали: {}", details) }
        );
        
        // Открываем файл в режиме добавления
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path) {
            
            // Записываем информацию о прогрессе
            let _ = file.write_all(log_entry.as_bytes());
        }
    }
}

/// Наблюдатель, отправляющий информацию о прогрессе через канал
pub struct ChannelProgressObserver {
    /// Отправитель для канала
    sender: mpsc::Sender<ProgressInfo>,
}

impl ChannelProgressObserver {
    /// Создать новый экземпляр ChannelProgressObserver
    pub fn new(sender: mpsc::Sender<ProgressInfo>) -> Self {
        Self { sender }
    }
}

impl ProgressObserver for ChannelProgressObserver {
    fn on_progress_update(&self, progress: ProgressInfo) {
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let _ = sender.send(progress).await;
        });
    }
}

/// Наблюдатель, вызывающий функцию обратного вызова при обновлении прогресса
pub struct CallbackProgressObserver<F>
where
    F: Fn(ProgressInfo) + Send + Sync + 'static,
{
    /// Функция обратного вызова
    callback: F,
}

impl<F> CallbackProgressObserver<F>
where
    F: Fn(ProgressInfo) + Send + Sync + 'static,
{
    /// Создать новый экземпляр CallbackProgressObserver
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

impl<F> ProgressObserver for CallbackProgressObserver<F>
where
    F: Fn(ProgressInfo) + Send + Sync + 'static,
{
    fn on_progress_update(&self, progress: ProgressInfo) {
        (self.callback)(progress);
    }
}

/// Наблюдатель, отображающий прогресс в виде прогресс-бара в консоли
pub struct ProgressBarObserver {
    /// Ширина прогресс-бара
    width: usize,
    /// Последний отображенный прогресс
    last_progress: Mutex<f32>,
}

impl ProgressBarObserver {
    /// Создать новый экземпляр ProgressBarObserver
    pub fn new(width: usize) -> Self {
        Self {
            width,
            last_progress: Mutex::new(-1.0), // Начальное значение, гарантирующее первое обновление
        }
    }
}

impl Default for ProgressBarObserver {
    fn default() -> Self {
        Self::new(50) // Стандартная ширина 50 символов
    }
}

impl ProgressObserver for ProgressBarObserver {
    fn on_progress_update(&self, progress: ProgressInfo) {
        let mut last_progress = self.last_progress.lock().unwrap();
        
        // Обновляем прогресс-бар только если изменение существенное (минимум 1%)
        // или это первое обновление, или прогресс достиг 100%
        if (*last_progress - progress.total_progress).abs() >= 1.0 || 
           *last_progress < 0.0 || 
           progress.total_progress >= 100.0 {
            
            *last_progress = progress.total_progress;
            
            // Вычисляем количество заполненных символов
            let filled = ((progress.total_progress / 100.0) * self.width as f32) as usize;
            let empty = self.width - filled;
            
            // Формируем строку прогресс-бара
            let bar = format!(
                "[{}{}] {:.1}% - {}",
                "=".repeat(filled),
                " ".repeat(empty),
                progress.total_progress,
                progress.step
            );
            
            // Выводим прогресс-бар, перезаписывая текущую строку
            print!("\r{}", bar);
            let _ = std::io::stdout().flush();
            
            // Если прогресс достиг 100%, добавляем перевод строки
            if progress.total_progress >= 100.0 {
                println!();
            }
        }
    }
}

/// Комбинированный наблюдатель, объединяющий несколько наблюдателей
pub struct CompositeProgressObserver {
    /// Список наблюдателей
    observers: Vec<Box<dyn ProgressObserver>>,
}

impl CompositeProgressObserver {
    /// Создать новый экземпляр CompositeProgressObserver
    pub fn new() -> Self {
        Self {
            observers: Vec::new(),
        }
    }
    
    /// Добавить наблюдателя
    pub fn add_observer(&mut self, observer: Box<dyn ProgressObserver>) {
        self.observers.push(observer);
    }
    
    /// Удалить всех наблюдателей
    pub fn clear(&mut self) {
        self.observers.clear();
    }
}

impl Default for CompositeProgressObserver {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressObserver for CompositeProgressObserver {
    fn on_progress_update(&self, progress: ProgressInfo) {
        for observer in &self.observers {
            observer.on_progress_update(progress.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    
    #[test]
    fn test_console_observer() {
        let observer = ConsoleProgressObserver::with_prefix("[Test] ");
        let progress = ProgressInfo::new("Test Step", 50.0, 25.0, Some("Testing".to_string()));
        
        // Этот тест просто проверяет, что метод не вызывает панику
        observer.on_progress_update(progress);
    }
    
    #[test]
    fn test_memory_observer() {
        let observer = MemoryProgressObserver::new();
        
        // Отправляем несколько обновлений
        observer.on_progress_update(ProgressInfo::new("Step 1", 50.0, 25.0, None));
        observer.on_progress_update(ProgressInfo::new("Step 1", 100.0, 50.0, None));
        observer.on_progress_update(ProgressInfo::new("Step 2", 50.0, 75.0, None));
        
        // Проверяем, что все обновления сохранены
        let history = observer.history();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].step, "Step 1");
        assert_eq!(history[1].step_progress, 100.0);
        assert_eq!(history[2].total_progress, 75.0);
        
        // Проверяем очистку истории
        observer.clear_history();
        assert_eq!(observer.history().len(), 0);
    }
    
    #[test]
    fn test_file_observer() {
        let temp_file = "test_progress.log";
        
        // Удаляем файл, если он существует
        if Path::new(temp_file).exists() {
            fs::remove_file(temp_file).unwrap();
        }
        
        let observer = FileProgressObserver::new(temp_file);
        observer.on_progress_update(ProgressInfo::new("Test Step", 50.0, 25.0, Some("Testing".to_string())));
        
        // Проверяем, что файл создан и содержит запись
        assert!(Path::new(temp_file).exists());
        let content = fs::read_to_string(temp_file).unwrap();
        assert!(content.contains("Test Step"));
        assert!(content.contains("50.0%"));
        assert!(content.contains("25.0%"));
        assert!(content.contains("Testing"));
        
        // Очищаем после теста
        fs::remove_file(temp_file).unwrap();
    }
    
    #[test]
    fn test_callback_observer() {
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();
        
        let observer = CallbackProgressObserver::new(move |_| {
            let mut count = counter_clone.lock().unwrap();
            *count += 1;
        });
        
        observer.on_progress_update(ProgressInfo::new("Step 1", 50.0, 25.0, None));
        observer.on_progress_update(ProgressInfo::new("Step 2", 0.0, 50.0, None));
        
        assert_eq!(*counter.lock().unwrap(), 2);
    }
    
    #[test]
    fn test_composite_observer() {
        let memory_observer = MemoryProgressObserver::new();
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();
        
        let callback_observer = CallbackProgressObserver::new(move |_| {
            let mut count = counter_clone.lock().unwrap();
            *count += 1;
        });
        
        let mut composite = CompositeProgressObserver::new();
        composite.add_observer(Box::new(memory_observer.clone()));
        composite.add_observer(Box::new(callback_observer));
        
        composite.on_progress_update(ProgressInfo::new("Step 1", 50.0, 25.0, None));
        
        // Проверяем, что оба наблюдателя получили уведомление
        assert_eq!(memory_observer.history().len(), 1);
        assert_eq!(*counter.lock().unwrap(), 1);
    }
}
