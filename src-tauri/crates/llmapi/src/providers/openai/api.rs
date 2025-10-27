use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::types::{ChatFn, LLMClient, LLMMessage, LLMMessageType, LLMUserType};
use crate::utils::detect_mime_type;

use super::models::{ChatCompletionResponse, ChatContent, ChatContentPart, EmbeddingResponse};

const OPENAI_MAX_TOKENS: u32 = 1024;

pub async fn chat(client: LLMClient) -> ChatFn {
    Arc::new(move |messages: Vec<LLMMessage>| {
        let client = client.clone();
        Box::pin(async move {
            match send_chat_completion(&client, messages).await {
                Ok(message) => message,
                Err(err) => {
                    eprintln!("OpenAI chat error: {err:?}");
                    LLMMessage::new(
                        None,
                        "System",
                        vec![LLMMessageType::text(format!("OpenAI error: {err}"))],
                    )
                }
            }
        })
    })
}

async fn send_chat_completion(client: &LLMClient, messages: Vec<LLMMessage>) -> Result<LLMMessage> {
    let url = format!(
        "{}/chat/completions",
        client.endpoint().trim_end_matches('/')
    );
    let payload = json!({
        "model": client.default_model(),
        "messages": convert_messages_to_openai(messages),
        "max_tokens": OPENAI_MAX_TOKENS
    });

    let http_client = Client::new();
    let response_text = http_client
        .post(url)
        .bearer_auth(client.api_key())
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .context("OpenAI request failed")?
        .error_for_status()
        .context("OpenAI returned non-success status")?
        .text()
        .await
        .context("Failed to read OpenAI response body")?;

    let response: ChatCompletionResponse = serde_json::from_str(&response_text)
        .with_context(|| format!("Failed to decode OpenAI response JSON: {response_text}"))?;

    convert_openai_response(response)
}

fn convert_messages_to_openai(messages: Vec<LLMMessage>) -> Vec<Value> {
    messages.into_iter().map(convert_message).collect()
}

fn convert_message(message: LLMMessage) -> Value {
    let role = match message.role {
        LLMUserType::Human => "user",
        LLMUserType::AI => "assistant",
        LLMUserType::System => "system",
    };

    let mut content_items = Vec::new();
    let mut text_segments = Vec::new();
    let mut only_text = true;

    for part in message.content {
        match part {
            LLMMessageType::TEXT(text) => {
                text_segments.push(text.clone());
                content_items.push(json!({
                    "type": "text",
                    "text": text
                }));
            }
            LLMMessageType::IMAGE {
                data_b64,
                file_path,
            } => {
                only_text = false;
                let mime = file_path
                    .as_deref()
                    .map(detect_mime_type)
                    .unwrap_or_else(|| "image/png".to_string());
                let data_url = format!("data:{mime};base64,{data_b64}");
                content_items.push(json!({
                    "type": "input_image",
                    "image_url": { "url": data_url }
                }));
            }
        }
    }

    if content_items.is_empty() {
        return json!({
            "role": role,
            "content": ""
        });
    }

    if only_text {
        let joined = text_segments.join("\n");
        json!({
            "role": role,
            "content": joined
        })
    } else {
        json!({
            "role": role,
            "content": content_items
        })
    }
}

fn convert_openai_response(response: ChatCompletionResponse) -> Result<LLMMessage> {
    let mut choices = response.choices.into_iter();
    let first_choice = choices
        .next()
        .ok_or_else(|| anyhow!("No choices returned from OpenAI"))?;

    let role = first_choice
        .message
        .role
        .clone()
        .unwrap_or_else(|| "assistant".to_string());

    let mut contents = match first_choice.message.content {
        Some(ChatContent::Text(text)) => vec![LLMMessageType::text(text)],
        Some(ChatContent::Parts(parts)) => convert_openai_parts(parts),
        None => Vec::new(),
    };

    if contents.is_empty() {
        contents.push(LLMMessageType::text("".to_string()));
    }

    Ok(LLMMessage::new(response.id, &role, contents))
}

fn convert_openai_parts(parts: Vec<ChatContentPart>) -> Vec<LLMMessageType> {
    let mut results = Vec::new();

    for part in parts {
        match part.kind.as_str() {
            "text" | "output_text" => {
                if let Some(text) = part.text {
                    results.push(LLMMessageType::text(text));
                }
            }
            "output_image" | "input_image" => {
                if let Some(image_base64) = part.image_base64 {
                    results.push(LLMMessageType::image_b64(image_base64));
                } else if let Some(image_url) = part.image_url {
                    if let Some(data_b64) = extract_data_url_base64(&image_url.url) {
                        results.push(LLMMessageType::image_b64(data_b64));
                    } else {
                        results.push(LLMMessageType::text(image_url.url));
                    }
                }
            }
            _ => {
                // Preserve unsupported content as text for visibility.
                let fallback = part
                    .text
                    .or_else(|| part.image_base64.clone())
                    .unwrap_or_else(|| format!("Unsupported OpenAI content type: {}", part.kind));
                results.push(LLMMessageType::text(fallback));
            }
        }
    }

    results
}

fn extract_data_url_base64(url: &str) -> Option<String> {
    let comma_idx = url.find(',')?;
    Some(url[(comma_idx + 1)..].to_string())
}

pub async fn openai_embed_texts(client: &LLMClient, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let url = format!("{}/embeddings", client.endpoint().trim_end_matches('/'));
    let payload = json!({
        "model": client.default_model(),
        "input": inputs,
    });

    let http_client = Client::new();
    let response = http_client
        .post(url)
        .bearer_auth(client.api_key())
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .context("OpenAI embeddings request failed")?;

    let status = response.status();
    let response_text = response
        .text()
        .await
        .context("Failed to read OpenAI embeddings response body")?;

    if !status.is_success() {
        return Err(anyhow!(
            "OpenAI embeddings failed: status {} body {}",
            status,
            response_text
        ));
    }

    let parsed: EmbeddingResponse = serde_json::from_str(&response_text)
        .with_context(|| format!("Failed to decode OpenAI embeddings JSON: {response_text}"))?;

    Ok(parsed.data.into_iter().map(|item| item.embedding).collect())
}
