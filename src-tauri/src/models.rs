use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateImageRequest {
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub image_prompt: String,
    #[serde(default)]
    pub reference_images: Vec<ReferenceImagePayload>,
    pub size: Option<String>,
    pub quality: Option<String>,
    pub style: Option<String>,
    pub user: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceImagePayload {
    pub mime_type: Option<String>,
    pub data_base64: String,
    pub slot: Option<String>,
    pub file_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GeneratedImage {
    pub mime_type: String,
    pub base64: String,
    pub revised_prompt: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredImage {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub mime_type: String,
    pub base64: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadImagePayload {
    pub file_name: String,
    pub mime_type: Option<String>,
    pub data_base64: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePromptsPayload {
    pub id: Option<String>,
    pub name: String,
    pub system_prompt: String,
    pub user_prompt: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedImageResponsePayload {
    pub image: StoredImage,
    pub revised_prompt: Option<String>,
}
