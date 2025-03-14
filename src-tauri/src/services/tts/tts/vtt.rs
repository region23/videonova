use std::path::Path;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use log::{info, warn, error};
use crate::errors::AppResult;
use crate::commands::speech_commands::Segment;

/// Парсит VTT файл и возвращает сегменты
pub fn parse_vtt(path: &Path) -> AppResult<Vec<Segment>> {
    info!("Parsing VTT file: {}", path.display());
    
    // Делегируем работу модулю vtt в корне services/tts
    crate::services::tts::vtt::parse_vtt(path)
} 