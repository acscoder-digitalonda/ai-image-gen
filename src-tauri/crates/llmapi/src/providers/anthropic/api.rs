use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::types::{ChatFn, LLMClient, LLMMessage, LLMMessageType, LLMUserType};
use crate::utils::detect_mime_type;

use super::models::{AnthropicContent, AnthropicResponse};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const ANTHROPIC_MAX_TOKENS: u32 = 1024;

pub async fn chat(client: LLMClient) -> ChatFn {
    Arc::new(move |messages: Vec<LLMMessage>| {
        let client = client.clone();
        Box::pin(async move {
            match send_message(&client, messages).await {
                Ok(message) => message,
                Err(err) => {
                    eprintln!("Anthropic chat error: {err:?}");
                    LLMMessage::new(
                        None,
                        "System",
                        vec![LLMMessageType::text(format!("Anthropic error: {err}"))],
                    )
                }
            }
        })
    })
}

async fn send_message(client: &LLMClient, messages: Vec<LLMMessage>) -> Result<LLMMessage> {
    let url = format!("{}/messages", client.endpoint().trim_end_matches('/'));

    let (anthropic_messages, system_prompt) = convert_messages_to_anthropic(messages);

    let mut payload = json!({
        "model": client.default_model(),
        "messages": anthropic_messages,
        "max_tokens": ANTHROPIC_MAX_TOKENS
    });

    if let Some(system) = system_prompt {
        payload["system"] = Value::String(system);
    }

    let http_client = Client::new();
    let response_text = http_client
        .post(url)
        .header("x-api-key", client.api_key())
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("Content-Type", "application/json")
        .header("accept", "application/json")
        .json(&payload)
        .send()
        .await
        .context("Anthropic request failed")?
        .error_for_status()
        .context("Anthropic returned non-success status")?
        .text()
        .await
        .context("Failed to read Anthropic response body")?;

    let response: AnthropicResponse = serde_json::from_str(&response_text)
        .with_context(|| format!("Failed to decode Anthropic response JSON: {response_text}"))?;

    convert_anthropic_response(response)
}

fn convert_messages_to_anthropic(messages: Vec<LLMMessage>) -> (Vec<Value>, Option<String>) {
    let mut system_segments = Vec::new();
    let mut converted = Vec::new();

    for message in messages {
        match message.role {
            LLMUserType::System => {
                let text = extract_text_from_message_content(message.content);
                if !text.is_empty() {
                    system_segments.push(text);
                }
            }
            role => {
                let role_str = match role {
                    LLMUserType::Human => "user",
                    LLMUserType::AI => "assistant",
                    LLMUserType::System => unreachable!("Handled above"),
                };

                let content = convert_message_content_to_anthropic(message.content);
                converted.push(json!({
                    "role": role_str,
                    "content": content
                }));
            }
        }
    }

    let system_prompt = if system_segments.is_empty() {
        None
    } else {
        Some(system_segments.join("\n"))
    };

    (converted, system_prompt)
}

fn extract_text_from_message_content(content: Vec<LLMMessageType>) -> String {
    let mut texts = Vec::new();
    for item in content {
        if let LLMMessageType::TEXT(text) = item {
            texts.push(text);
        }
    }
    texts.join("\n")
}

fn convert_message_content_to_anthropic(content: Vec<LLMMessageType>) -> Vec<Value> {
    let mut parts = Vec::new();

    for item in content {
        match item {
            LLMMessageType::TEXT(text) => parts.push(json!({
                "type": "text",
                "text": text
            })),
            LLMMessageType::IMAGE {
                data_b64,
                file_path,
            } => {
                let mime = file_path
                    .as_deref()
                    .map(detect_mime_type)
                    .unwrap_or_else(|| "image/png".to_string());
                parts.push(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": mime,
                        "data": data_b64
                    }
                }));
            }
        }
    }

    if parts.is_empty() {
        parts.push(json!({
            "type": "text",
            "text": ""
        }));
    }

    parts
}

fn convert_anthropic_response(response: AnthropicResponse) -> Result<LLMMessage> {
    let role = response
        .role
        .clone()
        .unwrap_or_else(|| "assistant".to_string());
    let mut contents = convert_anthropic_parts(response.content);

    if contents.is_empty() {
        contents.push(LLMMessageType::text("".to_string()));
    }

    Ok(LLMMessage::new(response.id, &role, contents))
}

fn convert_anthropic_parts(parts: Vec<AnthropicContent>) -> Vec<LLMMessageType> {
    let mut result = Vec::new();

    for part in parts {
        match part.kind.as_str() {
            "text" => {
                if let Some(text) = part.text {
                    result.push(LLMMessageType::text(text));
                }
            }
            "image" => {
                if let Some(source) = part.source {
                    if let Some(data) = source.data {
                        result.push(LLMMessageType::image_b64(data));
                    }
                }
            }
            _ => {
                let fallback = if !part.extra.is_null() {
                    part.extra.to_string()
                } else {
                    format!("Unsupported Anthropic content type: {}", part.kind)
                };
                result.push(LLMMessageType::text(fallback));
            }
        }
    }

    result
}

pub fn embedding(_client: LLMClient) -> Box<dyn Fn(&str) -> Vec<f32>> {
    Box::new(move |_input: &str| -> Vec<f32> { vec![0.4, 0.5, 0.6] })
}
