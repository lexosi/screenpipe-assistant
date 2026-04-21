use std::time::Duration;

use log::info;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct SendMessage<'a> {
    chat_id: &'a str,
    text: &'a str,
}

#[derive(Deserialize)]
pub struct Update {
    pub update_id: i64,
    pub message: Option<TgMessage>,
}

#[derive(Deserialize)]
pub struct TgMessage {
    pub text: Option<String>,
}

#[derive(Deserialize)]
struct UpdatesResponse {
    result: Vec<Update>,
}

pub fn send_message(
    bot_token: &str,
    chat_id: &str,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Sending Telegram message to chat {}", chat_id);

    let client = reqwest::blocking::Client::new();
    let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
    let body = SendMessage { chat_id, text };

    let response = client.post(&url).json(&body).send()?;

    if !response.status().is_success() {
        let status = response.status();
        let err_body = response.text().unwrap_or_default();
        return Err(format!("Telegram returned {}: {}", status, err_body).into());
    }

    info!("Telegram message delivered");
    Ok(())
}

/// Long-polls the Telegram Bot API for new updates.
/// `timeout_secs = 0` returns immediately (no long-poll); use for draining existing updates.
/// `timeout_secs > 0` blocks up to that many seconds waiting for a message.
pub fn get_updates(
    bot_token: &str,
    offset: i64,
    timeout_secs: u64,
) -> Result<Vec<Update>, Box<dyn std::error::Error>> {
    // HTTP timeout must exceed the long-poll timeout to avoid a spurious timeout error.
    let http_timeout = Duration::from_secs(timeout_secs + 10);
    let client = reqwest::blocking::Client::builder()
        .timeout(http_timeout)
        .build()?;

    let url = format!(
        "https://api.telegram.org/bot{}/getUpdates?offset={}&timeout={}",
        bot_token, offset, timeout_secs
    );

    let response = client.get(&url).send()?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("Telegram getUpdates returned {}: {}", status, body).into());
    }

    let resp: UpdatesResponse = response.json()?;
    Ok(resp.result)
}
