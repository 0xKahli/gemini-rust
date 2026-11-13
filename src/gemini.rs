use crate::types::{Message, Role};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc;

pub enum AuthMethod {
    ApiKey(String),
    Bearer(String),
}

pub async fn stream_message(
    auth: AuthMethod,
    model: String,
    history: Vec<Message>,
    tx: mpsc::Sender<String>,
) {
    let client = Client::new();

    let (url, request) = match &auth {
        AuthMethod::ApiKey(key) => {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
                model, key
            );
            (url, client.post(&url))
        }
        AuthMethod::Bearer(token) => {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse",
                model
            );
            (url.clone(), client.post(&url).bearer_auth(token))
        }
    };

    let contents: Vec<Value> = history
        .iter()
        .map(|msg| {
            let role = match msg.role {
                Role::User => "user",
                Role::Model => "model",
            };
            json!({
                "role": role,
                "parts": [{ "text": msg.content }]
            })
        })
        .collect();

    let body = json!({
        "contents": contents,
        "generationConfig": {
            "temperature": 1.0,
            "topK": 40,
            "topP": 0.95,
            "maxOutputTokens": 8192,
        }
    });

    let response = match request.json(&body).send().await {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(format!("[Error sending request: {}]", e)).await;
            return;
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let _ = tx.send(format!("[API Error {}: {}]", status, text)).await;
        return;
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(format!("[Stream error: {}]", e)).await;
                break;
            }
        };

        let text = match std::str::from_utf8(&chunk) {
            Ok(t) => t.to_string(),
            Err(_) => continue,
        };

        buffer.push_str(&text);

        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim().to_string();
            buffer = buffer[newline_pos + 1..].to_string();

            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    return;
                }
                if let Ok(json) = serde_json::from_str::<Value>(data) {
                    if let Some(token) = json
                        .pointer("/candidates/0/content/parts/0/text")
                        .and_then(|v| v.as_str())
                    {
                        if tx.send(token.to_string()).await.is_err() {
                            return;
                        }
                    }
                }
            }
        }
    }
}
