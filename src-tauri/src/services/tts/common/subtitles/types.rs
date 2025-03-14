use std::time::Duration;

/// Структура для представления одного субтитра из VTT
#[derive(Clone, Debug)]
pub struct SubtitleCue {
    /// Порядковый номер субтитра
    pub index: usize,
    /// Время начала
    pub start: Duration,
    /// Время окончания
    pub end: Duration,
    /// Текст субтитра
    pub text: String,
}

impl SubtitleCue {
    /// Создает новый субтитр
    pub fn new(index: usize, start: Duration, end: Duration, text: String) -> Self {
        Self {
            index,
            start,
            end,
            text,
        }
    }

    /// Получает длительность субтитра
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }
}

/// Результат парсинга VTT файла
#[derive(Debug)]
pub struct VttParseResult {
    /// Список субтитров
    pub cues: Vec<SubtitleCue>,
    /// Общая длительность
    pub duration: Duration,
}

/// Ошибки парсинга VTT
#[derive(Debug, thiserror::Error)]
pub enum VttError {
    #[error("Ошибка парсинга времени: {0}")]
    TimeParseError(String),
    
    #[error("Некорректный формат VTT: {0}")]
    InvalidFormat(String),
    
    #[error("Ошибка ввода/вывода: {0}")]
    IoError(#[from] std::io::Error),
} 