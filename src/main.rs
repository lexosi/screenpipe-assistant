mod claude;
mod config;
mod monitor;
mod telegram;

use std::fs::OpenOptions;
use std::path::PathBuf;

use log::{error, info};
use simplelog::{
    ColorChoice, CombinedLogger, Config as LogConfig, LevelFilter, TermLogger, TerminalMode,
    WriteLogger,
};

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .expect("Cannot determine executable path")
        .parent()
        .expect("Executable has no parent directory")
        .to_path_buf()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let exe_dir = exe_dir();
    let log_path = exe_dir.join("screenpipe-assistant.log");

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("Failed to open log file");

    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            LogConfig::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(LevelFilter::Info, LogConfig::default(), log_file),
    ])
    .expect("Failed to initialize logger");

    let config_path = exe_dir.join("config.toml");
    let cfg = match config::Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            error!("Configuration error: {}", e);
            std::process::exit(1);
        }
    };

    match args.get(1).map(String::as_str).unwrap_or("") {
        "run" => {
            info!("Starting screenpipe-assistant");
            if let Err(e) = monitor::run(&cfg, &exe_dir) {
                error!("Fatal monitor error: {}", e);
                std::process::exit(1);
            }
        }
        "test" => {
            info!("Sending test message to Telegram");
            match telegram::send_message(
                &cfg.telegram_bot_token,
                &cfg.telegram_chat_id,
                "Test message from screenpipe-assistant — Telegram connection is working!",
            ) {
                Ok(()) => info!("Test message delivered successfully"),
                Err(e) => {
                    error!("Test failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("Usage: screenpipe-assistant <run|test>");
            std::process::exit(1);
        }
    }
}
