use anyhow::{Context, Result};
use base64::Engine as _;
use reqwest::Client;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

use crate::types::{LLMClient, LLMMessage, LLMMessageType, LLMUserType};
use crate::utils::detect_mime_type;

use super::models::{GeminiBatchEmbedResponse, GeminiEmbedResponse, GeminiResponse, InlineData};

pub fn convert_body_parts_gemini(body_part: Vec<LLMMessageType>) -> Vec<Value> {
    body_part
        .into_iter()
        .map(|part| match part {
            LLMMessageType::TEXT(text) => json!({ "text": text }),
            LLMMessageType::IMAGE {
                data_b64,
                file_path,
            } => {
                let mime = file_path
                    .as_ref()
                    .map(detect_mime_type)
                    .unwrap_or_else(|| "image/jpeg".into());
                json!({
                    "inlineData": {
                        "mimeType": mime,
                        "data": data_b64
                    }
                })
            }
        })
        .collect()
}
pub fn convert_messages_to_gemini_contents(messages: Vec<LLMMessage>) -> Vec<Value> {
    messages
        .into_iter()
        .map(|m| {
            let parts = convert_body_parts_gemini(m.content);
            json!({
                "role": role_to_str(m.role),
                "parts": parts
            })
        })
        .collect()
}
fn role_to_str(role: LLMUserType) -> &'static str {
    match role {
        LLMUserType::Human => "user",
        LLMUserType::AI => "model",
        LLMUserType::System => "system",
    }
}
pub async fn send_generate_request(
    api_client: &LLMClient,
    body_part: Vec<LLMMessage>,
) -> Result<GeminiResponse> {
    let endpoint = api_client.endpoint().trim_end_matches('/');
    let url = format!(
        "{}/{}:generateContent",
        endpoint,
        api_client.default_model()
    );

    let body = json!({
        "contents": convert_messages_to_gemini_contents(body_part)
    });

    //log_request_payload(&url, &body);

    let client = Client::new();
    let response_text = client
        .post(url)
        .header("x-goog-api-key", api_client.api_key())
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("HTTP request failed")?
        .error_for_status()
        .context("Non-success status returned")?
        .text()
        .await
        .context("Reading response body failed")?;

    let response: GeminiResponse = serde_json::from_str(&response_text).with_context(|| {
        format!(
            "Failed to decode Gemini response JSON. Raw response: {}",
            response_text
        )
    })?;

    Ok(response)
}

fn log_request_payload(url: &str, body: &Value) {
    let log_dir = Path::new("test_data");
    if let Err(err) = fs::create_dir_all(log_dir) {
        eprintln!("Failed to create log directory {:?}: {}", log_dir, err);
        return;
    }
    let log_path = log_dir.join("last_gemini_request.json");
    let log_json = json!({
        "url": url,
        "body": body
    });
    let content = match serde_json::to_string_pretty(&log_json) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("Failed to serialize Gemini request log JSON: {}", err);
            return;
        }
    };
    if let Err(err) = fs::write(&log_path, content) {
        eprintln!("Failed to write Gemini request log {:?}: {}", log_path, err);
    }
}

fn decode_inline_data(inline_data: &InlineData) -> Result<Vec<u8>> {
    let decoded_data = base64::engine::general_purpose::STANDARD
        .decode(&inline_data.data)
        .context("Base64 decoding failed")?;
    Ok(decoded_data)
}
pub fn response_to_image_data(response: &GeminiResponse) -> Vec<Vec<u8>> {
    let images: Vec<Vec<u8>> = response
        .candidates
        .iter()
        .map(|candidate| {
            let r: Vec<Vec<u8>> = candidate
                .content
                .parts
                .iter()
                .map(|part| {
                    if let Some(inline_data) = &part.inline_data {
                        return decode_inline_data(inline_data).unwrap_or(vec![]);
                    } else {
                        return vec![];
                    }
                })
                .filter(|data| !data.is_empty())
                .collect();
            return r;
        })
        .flatten()
        .collect();

    images
}
pub fn response_to_base64_images(response: &GeminiResponse) -> Vec<String> {
    let images: Vec<String> = response
        .candidates
        .iter()
        .map(|candidate| {
            let r: Vec<String> = candidate
                .content
                .parts
                .iter()
                .map(|part| {
                    if let Some(inline_data) = &part.inline_data {
                        return inline_data.data.clone();
                    } else {
                        return "".to_owned();
                    }
                })
                .filter(|data| !data.is_empty())
                .collect();
            return r;
        })
        .flatten()
        .collect();

    images
}

pub fn response_to_text_data(response: &GeminiResponse) -> Result<String> {
    //get only 1 text response
    if let Some(candidate) = response.candidates.first() {
        let mut full_text = String::new();
        for part in &candidate.content.parts {
            if let Some(text) = &part.text {
                full_text.push_str(text);
            }
        }
        Ok(full_text)
    } else {
        Err(anyhow::anyhow!("No candidates found"))
    }
}

fn build_embed_content(text: &str, model: &str) -> Value {
    json!({
        "model": model,
        "content": {
            "parts": [{ "text": text }]
        }
    })
}

fn build_batch_embed_contents(texts: &[&str], model: &str) -> Value {
    json!({
        "requests": texts.iter().map(|t| {
            json!({
                "model": model,
                "content": { "parts": [{ "text": t }] }
            })
        }).collect::<Vec<_>>()
    })
}

pub async fn gemini_embed_texts(
    api_client: &LLMClient,
    texts: &Vec<String>,
) -> Result<Vec<Vec<f32>>> {
    let endpoint = api_client.endpoint().trim_end_matches('/');
    let model_id = api_client.default_model();
    let path_model = model_id.trim_start_matches("models/");
    let request_model = if model_id.starts_with("models/") {
        model_id.to_string()
    } else {
        format!("models/{}", model_id)
    };

    let client = Client::new();

    if texts.len() == 1 {
        // --- Single text ---
        let url = format!("{}/{path_model}:embedContent", endpoint);
        let body = build_embed_content(texts[0].as_ref(), &request_model);
        //log_request_payload(&url, &body);

        let response = client
            .post(&url)
            .header("x-goog-api-key", api_client.api_key())
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("HTTP request (embedContent) failed")?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .context("Reading embedContent response body failed")?;

        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Gemini embedContent failed: status {} body {}",
                status,
                response_text
            ));
        }

        let parsed: GeminiEmbedResponse =
            serde_json::from_str(&response_text).with_context(|| {
                format!("Failed to decode embedContent JSON. Raw: {}", response_text)
            })?;

        // Friendly guard: "usage only" or missing embedding
        let embedding = parsed.embedding.ok_or_else(|| {
            anyhow::anyhow!("Gemini responded with usage metadata only — no embedding produced")
        })?;

        return Ok(vec![embedding.values]);
    }

    // --- Batch multi-text ---
    let url = format!("{}/{path_model}:batchEmbedContents", endpoint);
    // Build & log
    let texts_ref: Vec<&str> = texts.iter().map(|t| t.as_ref()).collect();
    let body = build_batch_embed_contents(&texts_ref, &request_model);
    //log_request_payload(&url, &body);

    let response = client
        .post(&url)
        .header("x-goog-api-key", api_client.api_key())
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("HTTP request (batchEmbedContents) failed")?;

    let status = response.status();
    let response_text = response
        .text()
        .await
        .context("Reading batchEmbedContents response body failed")?;

    if !status.is_success() {
        return Err(anyhow::anyhow!(
            "Gemini batchEmbedContents failed: status {} body {}",
            status,
            response_text
        ));
    }

    let parsed: GeminiBatchEmbedResponse =
        serde_json::from_str(&response_text).with_context(|| {
            format!(
                "Failed to decode batchEmbedContents JSON. Raw: {}",
                response_text
            )
        })?;

    let embeddings = parsed.embeddings.ok_or_else(|| {
        anyhow::anyhow!("Gemini responded with usage metadata only — no embeddings produced")
    })?;

    Ok(embeddings.into_iter().map(|e| e.values).collect())
}
