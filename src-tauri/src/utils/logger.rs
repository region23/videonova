use env_logger::{Builder, Env};
use log::LevelFilter;
use std::io::Write;

pub fn init_logger() {
    // Set RUST_LOG explicitly for HTTP request tracing if not set
    if std::env::var("RUST_LOG").is_err() {
        // Use unsafe block for setting environment variables
        unsafe {
            std::env::set_var("RUST_LOG", "warn,videonova=info,tts_sync=debug,reqwest=debug,openai=trace");
        }
    }
    
    // Установка базового фильтра и переопределение через переменные окружения
    let env = Env::default().filter_or("RUST_LOG", "warn,videonova=info,tts_sync=debug,reqwest=debug,openai=trace");

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
        // Добавляем детальное логирование для tts-sync
        .filter_module("tts_sync", LevelFilter::Debug)
        .filter_module("tts_sync::tts::openai", LevelFilter::Trace)
        // Включаем логирование HTTP-клиента
        .filter_module("reqwest", LevelFilter::Debug)
        .filter_module("hyper::client", LevelFilter::Debug)
        .filter_module("rustls", LevelFilter::Debug)
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
