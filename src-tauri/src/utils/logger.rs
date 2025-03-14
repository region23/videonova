//! Application logging configuration.
//! 
//! This module provides functions to initialize and configure logging for the application.
//! It sets up appropriate log levels for different modules, filters out noisy logs from
//! dependencies, and configures formatting for log messages.

use env_logger::{Builder, Env};
use log::LevelFilter;
use std::io::Write;

/// Initialize and configure the application logger.
/// 
/// This function:
/// - Sets default log levels if not specified via environment variables
/// - Configures module-specific log levels
/// - Sets up log formatting
/// - Redirects logs to stderr for compatibility with Tauri console
/// 
/// # Example
/// ```
/// use videonova::utils::logger::init_logger;
/// 
/// fn main() {
///     init_logger();
///     log::info!("Application started");
/// }
/// ```
pub fn init_logger() {
    // Set RUST_LOG explicitly for HTTP request tracing if not set
    if std::env::var("RUST_LOG").is_err() {
        // Use unsafe block for setting environment variables
        unsafe {
            std::env::set_var("RUST_LOG", "warn,videonova=info,tts_sync=debug,reqwest=debug,openai=trace");
        }
    }
    
    // Set base filter and override through environment variables
    let env = Env::default().filter_or("RUST_LOG", "warn,videonova=info,tts_sync=debug,reqwest=debug,openai=trace");

    let mut builder = Builder::from_env(env);

    // Explicitly suppress logs from certain modules
    builder
        .filter_module("wry", LevelFilter::Error)
        .filter_module("tracing", LevelFilter::Error)
        .filter_module("mio", LevelFilter::Error)
        .filter_module("tokio_util", LevelFilter::Error)
        .filter_module("hyper", LevelFilter::Error)
        .filter_module("tauri", LevelFilter::Warn)
        .filter_module("tao", LevelFilter::Error)
        // Add detailed logging for tts-sync
        .filter_module("tts_sync", LevelFilter::Debug)
        .filter_module("tts_sync::tts::openai", LevelFilter::Trace)
        // Enable HTTP client logging
        .filter_module("reqwest", LevelFilter::Debug)
        .filter_module("hyper::client", LevelFilter::Debug)
        .filter_module("rustls", LevelFilter::Debug)
        // Allow DEBUG messages for the transcribe module
        .filter_module("videonova::utils::transcribe", LevelFilter::Debug)
        // Log formatting
        .format(|buf, record| {
            writeln!(
                buf,
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            )
        })
        .target(env_logger::Target::Stderr) // Output to stderr for compatibility with Tauri console
        .init();
}
