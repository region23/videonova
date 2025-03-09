use log::LevelFilter;
use env_logger::{Builder, Env};
use std::io::Write;

pub fn init_logger() {
    // Установка базового фильтра и переопределение через переменные окружения
    let env = Env::default().filter_or("RUST_LOG", "warn,yt_translator=info");
    
    let mut builder = Builder::from_env(env);
    
    // Явно подавляем логи от определенных модулей
    builder
        .filter_module("wry", LevelFilter::Error)
        .filter_module("tracing", LevelFilter::Error)
        .filter_module("mio", LevelFilter::Error)
        .filter_module("tokio_util", LevelFilter::Error)
        .filter_module("hyper", LevelFilter::Error)
        .filter_module("tauri", LevelFilter::Warn)
        .filter_module("tao", LevelFilter::Error)
        // Для модуля transcribe разрешаем также и DEBUG-сообщения
        .filter_module("videonova::utils::transcribe", LevelFilter::Debug)
        // Форматирование логов
        .format(|buf, record| {
            writeln!(
                buf,
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            )
        })
        .target(env_logger::Target::Stderr) // Вывод в stderr для совместимости с консолью Tauri
        .init();
} 