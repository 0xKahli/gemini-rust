use crate::types::{Message, Role};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc;

pub enum AuthMethod {
    #[allow(dead_code)]
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

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse",
        model
    );

    let request = match &auth {
        AuthMethod::ApiKey(key) => client.post(&url).header("x-goog-api-key", key),
        AuthMethod::Bearer(token) => client.post(&url).bearer_auth(token),
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

    let mut response = match request.json(&body).send().await {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(format!("[Error sending request: {}]", e)).await;
            return;
        }
    };

    // If OAuth token is rejected due to scope permissions, automatically fallback to GEMINI_API_KEY
    if response.status().as_u16() == 403 {
        if let Some(key) = crate::auth::get_gemini_api_key() {
            if let Ok(fallback_resp) = client
                .post(&url)
                .header("x-goog-api-key", &key)
                .json(&body)
                .send()
                .await
            {
                if fallback_resp.status().is_success() {
                    response = fallback_resp;
                }
            }
        }
    }

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if text.contains("limit: 0") && model.contains("gemini-4") {
            let _ = tx
                .send("[🔒 This model requires an active Gemini Pro / Advanced subscription. Free tier accounts have 0 quota for this model. Please select Gemini 4 (Hydrogen) or Gemini 3 Flash from the model selector.]".to_string())
                .await;
        } else if text.contains("ACCESS_TOKEN_SCOPE_INSUFFICIENT") {
            let _ = tx
                .send("[🔒 Authentication scope insufficient: Google OAuth access tokens do not have direct Generative Language API scopes by default. Please verify your GEMINI_API_KEY in Settings or environment variables.]".to_string())
                .await;
        } else {
            let _ = tx.send(format!("[API Error {}: {}]", status, text)).await;
        }
        return;
    }

    let mut stream = response.bytes_stream();
    let mut raw_buffer: Vec<u8> = Vec::with_capacity(8192);

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(format!("[Stream error: {}]", e)).await;
                break;
            }
        };

        raw_buffer.extend_from_slice(&chunk);

        while let Some(newline_pos) = raw_buffer.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = raw_buffer.drain(..=newline_pos).collect();
            let line = match std::str::from_utf8(&line_bytes) {
                Ok(s) => s.trim(),
                Err(_) => continue,
            };

            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    return;
                }
                if let Ok(json) = serde_json::from_str::<Value>(data) {
                    // Check if prompt was blocked by safety filters
                    if let Some(block_reason) = json
                        .pointer("/promptFeedback/blockReason")
                        .and_then(|v| v.as_str())
                    {
                        let _ = tx
                            .send(format!("\n\n[Prompt blocked: {}]", block_reason))
                            .await;
                    }

                    // Extract all text parts from candidates
                    if let Some(parts) = json
                        .pointer("/candidates/0/content/parts")
                        .and_then(|v| v.as_array())
                    {
                        for part in parts {
                            if let Some(token) = part.get("text").and_then(|v| v.as_str()) {
                                if !token.is_empty() && tx.send(token.to_string()).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }

                    // Check for unusual finish reason (e.g. SAFETY, RECITATION)
                    if let Some(finish_reason) = json
                        .pointer("/candidates/0/finishReason")
                        .and_then(|v| v.as_str())
                    {
                        if finish_reason != "STOP" && finish_reason != "MAX_TOKENS" {
                            let _ = tx
                                .send(format!("\n\n[Generation stopped: {}]", finish_reason))
                                .await;
                        }
                    }
                }
            }
        }
    }
}
