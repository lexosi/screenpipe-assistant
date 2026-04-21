use std::path::Path;
use std::thread;
use std::time::Duration;

use log::{error, info, warn};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::claude;
use crate::config::Config;
use crate::telegram;

#[derive(Serialize, Deserialize, Default)]
struct State {
    last_id: i64,
}

fn load_state(path: &Path) -> State {
    if path.exists() {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => State::default(),
        }
    } else {
        State::default()
    }
}

fn save_state(path: &Path, state: &State) {
    match serde_json::to_string(state) {
        Ok(json) => {
            if let Err(e) = std::fs::write(path, json) {
                error!("Failed to save state to {}: {}", path.display(), e);
            }
        }
        Err(e) => error!("Failed to serialize state: {}", e),
    }
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}

pub fn run(cfg: &Config, exe_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let state_path = exe_dir.join("state.json");
    let mut state = load_state(&state_path);

    info!(
        "Monitor started. Last processed id: {}. Poll interval: {}s",
        state.last_id, cfg.poll_interval_secs
    );

    loop {
        match poll(cfg, &mut state) {
            Ok(0) => {}
            Ok(n) => {
                save_state(&state_path, &state);
                info!("Processed {} new clipboard event(s)", n);
            }
            Err(e) => error!("Poll error: {}", e),
        }
        thread::sleep(Duration::from_secs(cfg.poll_interval_secs));
    }
}

fn poll(cfg: &Config, state: &mut State) -> Result<usize, Box<dyn std::error::Error>> {
    let conn = Connection::open(&cfg.db_path)?;

    let mut stmt = conn.prepare(
        "SELECT id, text_content FROM ui_events \
         WHERE event_type = 'clipboard' AND app_name = 'WindowsTerminal.exe' AND id > ?1 \
         ORDER BY id ASC",
    )?;

    let rows: Vec<(i64, String)> = stmt
        .query_map([state.last_id], |row| {
            let id: i64 = row.get(0)?;
            let content: Option<String> = row.get(1)?;
            Ok((id, content.unwrap_or_default()))
        })?
        .filter_map(|r| r.ok())
        .collect();

    for (id, content) in &rows {
        let text = truncate(content, cfg.max_clipboard_length);

        if content.len() > text.len() {
            warn!(
                "Event id={}: truncated content from {} to {} chars",
                id,
                content.chars().count(),
                cfg.max_clipboard_length
            );
        }

        info!(
            "Processing clipboard event id={} ({} chars)",
            id,
            text.chars().count()
        );

        match claude::send_to_claude(&cfg.anthropic_api_key, text) {
            Ok(reply) => {
                if let Err(e) = telegram::send_message(
                    &cfg.telegram_bot_token,
                    &cfg.telegram_chat_id,
                    &reply,
                ) {
                    error!("Telegram error for event id={}: {}", id, e);
                }
            }
            Err(e) => error!("Claude API error for event id={}: {}", id, e),
        }

        state.last_id = *id;
    }

    Ok(rows.len())
}
