use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: Option<String>,
    pub choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
pub struct ChatChoice {
    pub message: ChatMessage,
}

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<ChatContent>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ChatContent {
    Text(String),
    Parts(Vec<ChatContentPart>),
}

#[derive(Debug, Deserialize)]
pub struct ChatContentPart {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: Option<String>,
    #[serde(rename = "image_url")]
    pub image_url: Option<ChatContentImageUrl>,
    #[serde(rename = "image_base64")]
    pub image_base64: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChatContentImageUrl {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct EmbeddingResponse {
    pub data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
pub struct EmbeddingData {
    pub embedding: Vec<f32>,
}
