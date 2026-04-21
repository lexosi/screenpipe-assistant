use log::info;
use serde::{Deserialize, Serialize};

const SYSTEM_PROMPT: &str = "You are a terminal assistant. The user sends you output \
    from their Windows terminal. Suggest the next command to run, explain in 1 line why, \
    and put the command on a separate line prefixed with CMD:";

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<Message<'a>>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct Response {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

pub fn send_to_claude(api_key: &str, text: &str) -> Result<String, Box<dyn std::error::Error>> {
    info!("Sending {} chars to Claude API", text.len());

    let client = reqwest::blocking::Client::new();
    let payload = Request {
        model: "claude-haiku-4-5-20251001",
        max_tokens: 1024,
        system: SYSTEM_PROMPT,
        messages: vec![Message { role: "user", content: text }],
    };

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&payload)
        .send()?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("Claude API returned {}: {}", status, body).into());
    }

    let resp: Response = response.json()?;
    let reply = resp
        .content
        .into_iter()
        .find(|b| b.block_type == "text")
        .and_then(|b| b.text)
        .ok_or("No text block in Claude response")?;

    info!("Claude reply received ({} chars)", reply.len());
    Ok(reply)
}
