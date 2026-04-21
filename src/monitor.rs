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
    telegram_update_offset: i64,
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

    // Drain any existing Telegram updates so old messages don't trigger execution.
    if state.telegram_update_offset == 0 {
        match telegram::get_updates(&cfg.telegram_bot_token, 0, 0) {
            Ok(updates) => {
                if let Some(last) = updates.last() {
                    state.telegram_update_offset = last.update_id + 1;
                    save_state(&state_path, &state);
                    info!("Initialized Telegram offset to {}", state.telegram_update_offset);
                }
            }
            Err(e) => warn!("Could not initialize Telegram offset: {}", e),
        }
    }

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

        if content.chars().count() > cfg.max_clipboard_length {
            warn!(
                "Event id={}: truncated from {} to {} chars",
                id,
                content.chars().count(),
                cfg.max_clipboard_length
            );
        }

        info!("Processing clipboard event id={} ({} chars)", id, text.chars().count());

        match claude::send_to_claude(&cfg.anthropic_api_key, text) {
            Ok(reply) => {
                match telegram::send_message(&cfg.telegram_bot_token, &cfg.telegram_chat_id, &reply)
                {
                    Ok(()) => {
                        handle_confirmation(cfg, state, &reply);
                    }
                    Err(e) => error!("Telegram error for event id={}: {}", id, e),
                }
            }
            Err(e) => error!("Claude API error for event id={}: {}", id, e),
        }

        state.last_id = *id;
    }

    Ok(rows.len())
}

fn extract_cmd(reply: &str) -> Option<String> {
    reply
        .lines()
        .find(|l| l.trim_start().starts_with("CMD:"))
        .map(|l| l.trim_start().trim_start_matches("CMD:").trim().to_string())
}

fn execute_command(cfg: &Config, reply: &str) {
    let cmd = match extract_cmd(reply) {
        Some(c) if !c.is_empty() => c,
        _ => {
            warn!("No CMD: line in Claude reply, skipping execution");
            return;
        }
    };

    info!("Executing: {}", cmd);
    match std::process::Command::new("cmd").args(["/C", &cmd]).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("$ {}\n{}{}", cmd, stdout, stderr);
            let msg = truncate(combined.trim(), 4000);
            if let Err(e) =
                telegram::send_message(&cfg.telegram_bot_token, &cfg.telegram_chat_id, msg)
            {
                error!("Failed to send command output: {}", e);
            }
        }
        Err(e) => error!("Failed to execute '{}': {}", cmd, e),
    }
}

fn handle_confirmation(cfg: &Config, state: &mut State, claude_reply: &str) {
    use std::time::Instant;
    let deadline = Instant::now() + Duration::from_secs(60);

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now()).as_secs();
        if remaining == 0 {
            info!("Confirmation timeout; skipping execution");
            return;
        }

        let poll_secs = remaining.min(30);
        match telegram::get_updates(&cfg.telegram_bot_token, state.telegram_update_offset, poll_secs)
        {
            Ok(updates) => {
                for update in updates {
                    state.telegram_update_offset = update.update_id + 1;
                    if let Some(msg) = update.message {
                        if let Some(text) = msg.text {
                            let reply = text.trim().to_lowercase();
                            if reply == "si" || reply == "yes" || reply == "s" {
                                info!("User confirmed; executing command");
                                execute_command(cfg, claude_reply);
                            } else {
                                info!("User declined (got: {:?}); skipping", text.trim());
                            }
                            return;
                        }
                    }
                }
            }
            Err(e) => {
                error!("Telegram getUpdates error: {}", e);
                thread::sleep(Duration::from_secs(5));
            }
        }
    }
}
