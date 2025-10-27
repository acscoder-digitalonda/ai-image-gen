use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64_ENGINE;
use base64::Engine;
use llmapi::providers::gemini::models::GeminiResponse;
use llmapi::providers::gemini::send_generate_request;
use llmapi::types::{LLMClient, LLMMessage, LLMMessageType, LLMProvider, LLMType, LLMUserType};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::fs::try_exists;

use crate::constants::{DEFAULT_GEMINI_ENDPOINT, DEFAULT_IMAGE_MIME, DEFAULT_IMAGE_MODEL};
use crate::fs_utils::{
    build_stored_image, default_extension_for_mime, ensure_output_dir, ensure_unique_file_name,
};
use crate::models::{GenerateImageRequest, GeneratedImage, GeneratedImageResponsePayload};

#[tauri::command]
pub async fn generate_image(
    payload: GenerateImageRequest,
) -> Result<GeneratedImageResponsePayload, String> {
    if payload.image_prompt.trim().is_empty() {
        return Err("Image prompt cannot be empty".into());
    }

    let api_key = payload
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "API key is required to generate images.".to_string())?;

    let model_name = payload
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_IMAGE_MODEL);

    let trimmed_model = model_name
        .strip_prefix("models/")
        .unwrap_or(model_name)
        .to_string();

    let mut messages: Vec<LLMMessage> = Vec::new();

    if let Some(system_prompt) = payload
        .system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        messages.push(LLMMessage::new(
            None,
            "Human",
            vec![LLMMessageType::text(system_prompt.to_string())],
        ));
    }

    let mut user_content: Vec<LLMMessageType> = Vec::new();

    for (index, reference) in payload.reference_images.iter().enumerate() {
        let data = reference.data_base64.trim();
        if data.is_empty() {
            continue;
        }

        let mime_type = reference
            .mime_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_IMAGE_MIME);

        let extension = default_extension_for_mime(mime_type).unwrap_or_else(|| "bin".to_string());
        let slot_name = reference
            .slot
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("reference_{index}"));

        let pseudo_path = if extension.is_empty() {
            slot_name
        } else {
            format!("{slot_name}.{extension}")
        };

        user_content.push(LLMMessageType::IMAGE {
            data_b64: data.to_string(),
            file_path: Some(pseudo_path),
        });
    }

    let user_prompt = build_user_prompt(&payload);
    if !user_prompt.trim().is_empty() {
        user_content.push(LLMMessageType::text(user_prompt));
    }

    if user_content.is_empty() {
        return Err("A prompt or reference image is required to generate content.".into());
    }

    messages.push(LLMMessage::new(None, "Human", user_content));

    let client = LLMClient::new(
        LLMProvider::Gemini,
        api_key,
        DEFAULT_GEMINI_ENDPOINT,
        trimmed_model.clone(),
        LLMType::Chat,
    );

    log_generate_payload(api_key, &trimmed_model, &messages)
        .await
        .map_err(|err| format!("Failed to write debug log: {}", err))?;

    let response = send_generate_request(&client, messages.clone())
        .await
        .map_err(|err| format!("Failed to request image generation: {}", err))?;

    let generated = extract_generated_image(response)?;

    let output_dir = ensure_output_dir().await?;
    let bytes = BASE64_ENGINE
        .decode(generated.base64.trim())
        .map_err(|err| format!("Failed to decode generated image: {}", err))?;

    let extension =
        default_extension_for_mime(&generated.mime_type).unwrap_or_else(|| "bin".to_string());
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let base_name = if extension.is_empty() {
        format!("image_{timestamp}")
    } else {
        format!("image_{timestamp}.{}", extension)
    };

    let unique_name = ensure_unique_file_name(&output_dir, &base_name).await?;
    let target_path = output_dir.join(&unique_name);

    fs::write(&target_path, &bytes)
        .await
        .map_err(|err| format!("Unable to persist generated image: {}", err))?;

    let stored_image = build_stored_image(
        &target_path,
        bytes.len() as u64,
        Some(generated.mime_type.clone()),
    )
    .await?;

    append_generation_log(GenerationLogEntry {
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        prompt: payload.image_prompt.trim().to_string(),
        system_prompt: payload
            .system_prompt
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        reference_images: payload
            .reference_images
            .iter()
            .filter_map(|reference| reference.file_name.as_ref())
            .map(|name| format!("input/{name}"))
            .collect(),
        output_image: format!("output/{}", stored_image.name.clone()),
    })
    .await?;

    Ok(GeneratedImageResponsePayload {
        image: stored_image,
        revised_prompt: generated.revised_prompt,
    })
}

const LOG_FILE_NAME: &str = "log.json";
const MAX_LOG_ENTRIES: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationLogEntry {
    pub timestamp: u64,
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub reference_images: Vec<String>,
    pub output_image: String,
}

async fn append_generation_log(entry: GenerationLogEntry) -> Result<(), String> {
    let dir = ensure_output_dir().await?;
    let path = dir.join(LOG_FILE_NAME);

    let mut entries: Vec<GenerationLogEntry> = if try_exists(&path)
        .await
        .map_err(|err| format!("Failed to check log file: {}", err))?
    {
        let contents = fs::read_to_string(&path)
            .await
            .unwrap_or_else(|_| "[]".to_string());
        serde_json::from_str(&contents).unwrap_or_default()
    } else {
        Vec::new()
    };

    entries.push(entry);
    if entries.len() > MAX_LOG_ENTRIES {
        entries = entries.split_off(entries.len() - MAX_LOG_ENTRIES);
    }

    let payload = serde_json::to_string_pretty(&entries)
        .map_err(|err| format!("Unable to serialise generation logs: {}", err))?;

    fs::write(&path, payload)
        .await
        .map_err(|err| format!("Failed to write generation log: {}", err))
}

#[tauri::command]
pub async fn list_generation_logs() -> Result<Vec<GenerationLogEntry>, String> {
    let dir = ensure_output_dir().await?;
    let path = dir.join(LOG_FILE_NAME);

    if !try_exists(&path)
        .await
        .map_err(|err| format!("Failed to check log file: {}", err))?
    {
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(&path)
        .await
        .map_err(|err| format!("Unable to read generation log: {}", err))?;

    serde_json::from_str(&contents)
        .map_err(|err| format!("Unable to parse generation log: {}", err))
}

fn build_user_prompt(payload: &GenerateImageRequest) -> String {
    let mut sections: Vec<String> = Vec::new();

    let trimmed_prompt = payload.image_prompt.trim();
    if !trimmed_prompt.is_empty() {
        sections.push(trimmed_prompt.to_string());
    }

    let mut details = Vec::new();

    if let Some(style) = payload
        .style
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        details.push(format!("Preferred style: {style}"));
    }

    if let Some(quality) = payload
        .quality
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        details.push(format!("Desired quality: {quality}"));
    }

    if let Some(size) = payload
        .size
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        details.push(format!("Target dimensions or aspect ratio: {size}"));
    }

    if let Some(user) = payload
        .user
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        details.push(format!("Requested by user: {user}"));
    }

    if !details.is_empty() {
        sections.push(details.join("\n"));
    }

    sections.join("\n\n")
}

fn extract_generated_image(response: GeminiResponse) -> Result<GeneratedImage, String> {
    for candidate in response.candidates {
        let mut first_text: Option<String> = None;

        for part in candidate.content.parts {
            if let Some(inline_data) = part.inline_data {
                let mime_type = inline_data.mime_type.trim();
                let mime_type = if mime_type.is_empty() {
                    DEFAULT_IMAGE_MIME
                } else {
                    mime_type
                };

                let data = inline_data.data.trim();
                if data.is_empty() {
                    continue;
                }

                return Ok(GeneratedImage {
                    mime_type: mime_type.to_string(),
                    base64: data.to_string(),
                    revised_prompt: first_text,
                });
            }

            if let Some(text) = part.text {
                let trimmed = text.trim();
                if !trimmed.is_empty() && first_text.is_none() {
                    first_text = Some(trimmed.to_string());
                }
            }
        }
    }

    Err("Provider did not return an image payload.".to_string())
}

async fn log_generate_payload(
    api_key: &str,
    model_name: &str,
    messages: &[LLMMessage],
) -> Result<(), String> {
    #[derive(Serialize)]
    struct DebugMessageContent {
        kind: &'static str,
        text: Option<String>,
        file_path: Option<String>,
        data_preview: Option<String>,
        data_length: Option<usize>,
    }

    #[derive(Serialize)]
    struct DebugMessage {
        id: String,
        role: String,
        created_at: i64,
        content: Vec<DebugMessageContent>,
    }

    #[derive(Serialize)]
    struct DebugPayload {
        api_key: String,
        model_name: String,
        message_count: usize,
        messages: Vec<DebugMessage>,
    }

    let log_messages: Vec<DebugMessage> = messages
        .iter()
        .map(|message| {
            let role = match message.role {
                LLMUserType::Human => "Human",
                LLMUserType::AI => "AI",
                LLMUserType::System => "System",
            };

            let content = message
                .content
                .iter()
                .map(|part| match part {
                    LLMMessageType::TEXT(text) => DebugMessageContent {
                        kind: "text",
                        text: Some(text.clone()),
                        file_path: None,
                        data_preview: None,
                        data_length: None,
                    },
                    LLMMessageType::IMAGE {
                        data_b64,
                        file_path,
                    } => {
                        let preview: String = data_b64.chars().take(80).collect();
                        DebugMessageContent {
                            kind: "image",
                            text: None,
                            file_path: file_path.clone(),
                            data_preview: Some(preview),
                            data_length: Some(data_b64.len()),
                        }
                    }
                })
                .collect();

            DebugMessage {
                id: message.id.clone(),
                role: role.to_string(),
                created_at: message.created_at,
                content,
            }
        })
        .collect();

    let payload = DebugPayload {
        api_key: api_key.to_string(),
        model_name: model_name.to_string(),
        message_count: log_messages.len(),
        messages: log_messages,
    };

    let mut dir = std::env::temp_dir();
    dir.push("image-gen-debug");

    if !try_exists(&dir).await.map_err(|err| err.to_string())? {
        fs::create_dir_all(&dir)
            .await
            .map_err(|err| err.to_string())?;
    }

    let log_path = dir.join("last_generate_image_request.json");
    let serialized = serde_json::to_string_pretty(&payload).map_err(|err| err.to_string())?;
    fs::write(log_path, serialized)
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}
