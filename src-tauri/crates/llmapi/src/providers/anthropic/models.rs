use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct AnthropicResponse {
    pub id: Option<String>,
    pub role: Option<String>,
    pub content: Vec<AnthropicContent>,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicContent {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: Option<String>,
    pub source: Option<AnthropicImageSource>,
    #[serde(default)]
    pub extra: Value,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicImageSource {
    pub data: Option<String>,
}
