use log::info;
use serde::Serialize;

#[derive(Serialize)]
struct SendMessage<'a> {
    chat_id: &'a str,
    text: &'a str,
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
