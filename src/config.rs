use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
pub struct Config {
    pub db_path: String,
    pub anthropic_api_key: String,
    pub telegram_bot_token: String,
    pub telegram_chat_id: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    #[serde(default = "default_max_clipboard_length")]
    pub max_clipboard_length: usize,
}

fn default_poll_interval() -> u64 {
    3
}

fn default_max_clipboard_length() -> usize {
    2000
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Cannot read config at {}: {}", path.display(), e))?;
        let config: Config = toml::from_str(&content)
            .map_err(|e| format!("Failed to parse config.toml: {}", e))?;
        Ok(config)
    }
}
