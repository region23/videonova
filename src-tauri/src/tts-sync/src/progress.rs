//! Модуль для отслеживания прогресса выполнения операций
//! 
//! Этот модуль предоставляет реализацию паттерна Observer для асинхронного
//! отслеживания прогресса выполнения длительных операций в библиотеке tts-sync.

use std::collections::HashMap;
use std::sync::{Arc, RwLock, atomic::{AtomicUsize, Ordering}};
use tokio::sync::broadcast;
use serde::{Serialize, Deserialize};

/// Информация о прогрессе выполнения операции
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressInfo {
    /// Текущий этап операции
    pub step: String,
    /// Процент выполнения текущего этапа (0.0 - 100.0)
    pub step_progress: f32,
    /// Общий процент выполнения всей операции (0.0 - 100.0)
    pub total_progress: f32,
    /// Дополнительная информация о текущем этапе
    pub details: Option<String>,
}

impl ProgressInfo {
    /// Создает новый экземпляр ProgressInfo
    pub fn new(step: impl Into<String>, step_progress: f32, total_progress: f32, details: Option<String>) -> Self {
        Self {
            step: step.into(),
            step_progress: step_progress.clamp(0.0, 100.0),
            total_progress: total_progress.clamp(0.0, 100.0),
            details,
        }
    }
}

/// Трейт для наблюдателя, получающего уведомления о прогрессе
pub trait ProgressObserver: Send + Sync {
    /// Метод, вызываемый при обновлении прогресса
    fn on_progress_update(&self, progress: ProgressInfo);
}

/// Трейт для объекта, отправляющего уведомления о прогрессе
pub trait ProgressReporter: Send + Sync {
    /// Добавить наблюдателя
    /// 
    /// Возвращает уникальный идентификатор наблюдателя, который можно использовать
    /// для его удаления в будущем.
    fn add_observer(&mut self, observer: Box<dyn ProgressObserver>) -> usize;
    
    /// Удалить наблюдателя по идентификатору
    /// 
    /// Возвращает удаленного наблюдателя, если он был найден.
    fn remove_observer(&mut self, id: usize) -> Option<Box<dyn ProgressObserver>>;
    
    /// Уведомить всех наблюдателей о прогрессе
    fn notify_progress(&self, progress: ProgressInfo);
}

/// Реализация ProgressReporter для отслеживания прогресса
pub struct DefaultProgressReporter {
    /// Список наблюдателей
    observers: RwLock<HashMap<usize, Box<dyn ProgressObserver>>>,
    /// Счетчик для генерации уникальных идентификаторов наблюдателей
    next_id: AtomicUsize,
}

impl DefaultProgressReporter {
    /// Создать новый экземпляр DefaultProgressReporter
    pub fn new() -> Self {
        Self {
            observers: RwLock::new(HashMap::new()),
            next_id: AtomicUsize::new(0),
        }
    }
    
    /// Получить следующий уникальный идентификатор
    fn next_id(&self) -> usize {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }
}

impl Default for DefaultProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressReporter for DefaultProgressReporter {
    fn add_observer(&mut self, observer: Box<dyn ProgressObserver>) -> usize {
        let id = self.next_id();
        let mut observers = self.observers.write().unwrap();
        observers.insert(id, observer);
        id
    }
    
    fn remove_observer(&mut self, id: usize) -> Option<Box<dyn ProgressObserver>> {
        let mut observers = self.observers.write().unwrap();
        observers.remove(&id)
    }
    
    fn notify_progress(&self, progress: ProgressInfo) {
        let observers = self.observers.read().unwrap();
        for observer in observers.values() {
            observer.on_progress_update(progress.clone());
        }
    }
}

/// Асинхронный репортер прогресса, использующий каналы Tokio
pub struct AsyncProgressReporter {
    /// Канал для отправки уведомлений о прогрессе
    tx: broadcast::Sender<ProgressInfo>,
    /// Внутренний репортер для хранения наблюдателей
    inner: DefaultProgressReporter,
}

impl AsyncProgressReporter {
    /// Создать новый асинхронный репортер прогресса
    pub fn new() -> (Self, broadcast::Receiver<ProgressInfo>) {
        let (tx, rx) = broadcast::channel(100);
        let reporter = Self {
            tx,
            inner: DefaultProgressReporter::new(),
        };
        (reporter, rx)
    }
    
    /// Запустить обработчик сообщений о прогрессе
    pub fn start_handler(self: Arc<Self>) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let mut rx = tx.subscribe();
            while let Ok(progress) = rx.recv().await {
                let observers = self.inner.observers.read().unwrap();
                for observer in observers.values() {
                    observer.on_progress_update(progress.clone());
                }
            }
        });
    }
}

impl ProgressReporter for AsyncProgressReporter {
    fn add_observer(&mut self, observer: Box<dyn ProgressObserver>) -> usize {
        self.inner.add_observer(observer)
    }
    
    fn remove_observer(&mut self, id: usize) -> Option<Box<dyn ProgressObserver>> {
        self.inner.remove_observer(id)
    }
    
    fn notify_progress(&self, progress: ProgressInfo) {
        // Отправляем прогресс в канал
        if let Err(e) = self.tx.send(progress.clone()) {
            log::error!("Failed to send progress update: {}", e);
        }
    }
}

/// Этапы процесса синхронизации TTS
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessStep {
    /// Парсинг и анализ субтитров
    SubtitleParsing,
    /// Анализ временных меток
    TimingAnalysis,
    /// Оптимизация субтитров для TTS
    SubtitleOptimization,
    /// Генерация речи с использованием OpenAI API
    SpeechGeneration,
    /// Синхронизация аудио с видео
    AudioVideoSync,
}

impl ProcessStep {
    /// Получить название этапа в виде строки
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SubtitleParsing => "Парсинг и анализ субтитров",
            Self::TimingAnalysis => "Анализ временных меток",
            Self::SubtitleOptimization => "Оптимизация субтитров для TTS",
            Self::SpeechGeneration => "Генерация речи",
            Self::AudioVideoSync => "Синхронизация аудио с видео",
        }
    }
    
    /// Получить весовой коэффициент этапа (в процентах от общего процесса)
    pub fn weight(&self) -> f32 {
        match self {
            Self::SubtitleParsing => 5.0,
            Self::TimingAnalysis => 5.0,
            Self::SubtitleOptimization => 10.0,
            Self::SpeechGeneration => 60.0,
            Self::AudioVideoSync => 20.0,
        }
    }
}

/// Трекер прогресса для отслеживания выполнения процесса
pub struct ProgressTracker {
    /// Репортер прогресса
    reporter: Option<Box<dyn ProgressReporter>>,
    /// Текущий этап
    current_step: RwLock<ProcessStep>,
    /// Прогресс текущего этапа (0.0 - 100.0)
    step_progress: RwLock<f32>,
    /// Общий прогресс (0.0 - 100.0)
    total_progress: RwLock<f32>,
    /// Завершенные этапы
    completed_steps: RwLock<HashMap<ProcessStep, f32>>,
}

impl ProgressTracker {
    /// Создать новый экземпляр ProgressTracker
    pub fn new() -> Self {
        Self {
            reporter: None,
            current_step: RwLock::new(ProcessStep::SubtitleParsing),
            step_progress: RwLock::new(0.0),
            total_progress: RwLock::new(0.0),
            completed_steps: RwLock::new(HashMap::new()),
        }
    }
    
    /// Создать новый экземпляр ProgressTracker с репортером
    pub fn with_reporter(reporter: Box<dyn ProgressReporter>) -> Self {
        Self {
            reporter: Some(reporter),
            current_step: RwLock::new(ProcessStep::SubtitleParsing),
            step_progress: RwLock::new(0.0),
            total_progress: RwLock::new(0.0),
            completed_steps: RwLock::new(HashMap::new()),
        }
    }
    
    /// Установить репортер прогресса
    pub fn set_reporter(&mut self, reporter: Box<dyn ProgressReporter>) {
        self.reporter = Some(reporter);
    }
    
    /// Добавить наблюдателя
    pub fn add_observer(&mut self, observer: Box<dyn ProgressObserver>) -> Option<usize> {
        self.reporter.as_mut().map(|reporter| reporter.add_observer(observer))
    }
    
    /// Установить текущий этап
    pub fn set_step(&self, step: ProcessStep) {
        // Если этап меняется, считаем предыдущий этап завершенным на 100%
        let mut current_step = self.current_step.write().unwrap();
        if *current_step != step {
            let mut completed_steps = self.completed_steps.write().unwrap();
            completed_steps.insert(*current_step, 100.0);
            *current_step = step;
            drop(completed_steps);
            
            let mut step_progress = self.step_progress.write().unwrap();
            *step_progress = 0.0;
            drop(step_progress);
            
            self.update_total_progress();
            self.report_progress(None);
        }
    }
    
    /// Обновить прогресс текущего этапа
    pub fn update_step_progress(&self, progress: f32, details: Option<String>) {
        let mut step_progress = self.step_progress.write().unwrap();
        *step_progress = progress.clamp(0.0, 100.0);
        drop(step_progress);
        
        self.update_total_progress();
        self.report_progress(details);
    }
    
    /// Обновить общий прогресс на основе прогресса этапов
    fn update_total_progress(&self) {
        let mut total = 0.0;
        let mut total_weight = 0.0;
        
        // Учитываем завершенные этапы
        let completed_steps = self.completed_steps.read().unwrap();
        for (step, progress) in completed_steps.iter() {
            total += step.weight() * progress / 100.0;
            total_weight += step.weight();
        }
        drop(completed_steps);
        
        // Учитываем текущий этап
        let current_step = self.current_step.read().unwrap();
        let step_progress = self.step_progress.read().unwrap();
        total += current_step.weight() * *step_progress / 100.0;
        total_weight += current_step.weight();
        
        // Рассчитываем общий прогресс
        let mut total_progress = self.total_progress.write().unwrap();
        *total_progress = (total / total_weight * 100.0).clamp(0.0, 100.0);
    }
    
    /// Отправить уведомление о прогрессе
    fn report_progress(&self, details: Option<String>) {
        if let Some(reporter) = &self.reporter {
            let current_step = self.current_step.read().unwrap();
            let step_progress = self.step_progress.read().unwrap();
            let total_progress = self.total_progress.read().unwrap();
            
            let progress = ProgressInfo::new(
                current_step.as_str(),
                *step_progress,
                *total_progress,
                details,
            );
            reporter.notify_progress(progress);
        }
    }
    
    /// Отметить завершение всего процесса
    pub fn complete(&self) {
        let current_step = self.current_step.read().unwrap();
        let mut completed_steps = self.completed_steps.write().unwrap();
        completed_steps.insert(*current_step, 100.0);
        drop(completed_steps);
        
        let mut total_progress = self.total_progress.write().unwrap();
        *total_progress = 100.0;
        
        self.report_progress(Some("Процесс завершен".to_string()));
    }

    /// Уведомить о прогрессе без изменения состояния
    pub fn notify_progress(&self, progress: f32, details: Option<String>) {
        if let Some(reporter) = &self.reporter {
            let current_step = self.current_step.read().unwrap();
            let total_progress = self.total_progress.read().unwrap();
            
            let progress_info = ProgressInfo::new(
                current_step.as_str(),
                progress,
                *total_progress,
                details,
            );
            reporter.notify_progress(progress_info);
        }
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    
    struct TestObserver {
        updates: Arc<Mutex<Vec<ProgressInfo>>>,
    }
    
    impl TestObserver {
        fn new() -> (Self, Arc<Mutex<Vec<ProgressInfo>>>) {
            let updates = Arc::new(Mutex::new(Vec::new()));
            (Self { updates: updates.clone() }, updates)
        }
    }
    
    impl ProgressObserver for TestObserver {
        fn on_progress_update(&self, progress: ProgressInfo) {
            let mut updates = self.updates.lock().unwrap();
            updates.push(progress);
        }
    }
    
    #[test]
    fn test_progress_tracker() {
        let mut tracker = ProgressTracker::new();
        let mut reporter = DefaultProgressReporter::new();
        
        let (observer, updates) = TestObserver::new();
        reporter.add_observer(Box::new(observer));
        
        tracker.set_reporter(Box::new(reporter));
        
        // Тестируем обновление прогресса
        tracker.update_step_progress(50.0, None);
        
        {
            let updates = updates.lock().unwrap();
            assert_eq!(updates.len(), 1);
            assert_eq!(updates[0].step, ProcessStep::SubtitleParsing.as_str());
            assert_eq!(updates[0].step_progress, 50.0);
            assert!(updates[0].total_progress > 0.0);
        }
        
        // Тестируем смену этапа
        tracker.set_step(ProcessStep::TimingAnalysis);
        
        {
            let updates = updates.lock().unwrap();
            assert_eq!(updates.len(), 2);
            assert_eq!(updates[1].step, ProcessStep::TimingAnalysis.as_str());
            assert_eq!(updates[1].step_progress, 0.0);
        }
        
        // Тестируем завершение процесса
        tracker.complete();
        
        {
            let updates = updates.lock().unwrap();
            assert_eq!(updates.len(), 3);
            assert_eq!(updates[2].total_progress, 100.0);
            assert_eq!(updates[2].details, Some("Процесс завершен".to_string()));
        }
    }
}
