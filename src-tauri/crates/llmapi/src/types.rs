use crate::utils;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub enum LLMProvider {
    OpenAI,
    Anthropic,
    Gemini,
}

#[derive(Clone, Copy)]
pub enum LLMType {
    Chat,
    Embedding,
}

#[derive(Clone, Debug)]
pub enum LLMMessageType {
    TEXT(String),
    IMAGE {
        data_b64: String,
        file_path: Option<String>,
    },
}
impl LLMMessageType {
    pub fn text(text: impl Into<String>) -> Self {
        LLMMessageType::TEXT(text.into())
    }
    pub fn image_b64(data_b64: impl Into<String>) -> Self {
        LLMMessageType::IMAGE {
            data_b64: data_b64.into(),
            file_path: None,
        }
    }
    pub fn image(path_str: impl Into<String>) -> Self {
        let path_str = path_str.into();
        let data_b64 = crate::utils::encode_image_to_base64(&path_str).unwrap_or_default();
        LLMMessageType::IMAGE {
            data_b64,
            file_path: Some(path_str),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LLMUserType {
    Human,
    AI,
    System,
}
impl LLMUserType {
    pub fn from_str(role_str: &str) -> Option<Self> {
        match role_str.trim().to_lowercase().as_str() {
            "user" | "human" => Some(LLMUserType::Human),
            "model" | "ai" | "assistant" => Some(LLMUserType::AI),
            "system" => Some(LLMUserType::System),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LLMMessage {
    pub id: String,
    pub role: LLMUserType,
    pub content: Vec<LLMMessageType>,
    pub created_at: i64,
}

impl LLMMessage {
    pub fn new(id: Option<String>, role: &str, content: Vec<LLMMessageType>) -> Self {
        let id = id.unwrap_or_else(|| utils::current_timestamp_millis().to_string());
        Self {
            id,
            role: LLMUserType::from_str(role).unwrap_or(LLMUserType::Human),
            content,
            created_at: utils::current_timestamp_millis() as i64,
        }
    }
}

#[derive(Clone)]
pub struct LLMClient {
    pub(crate) provider: LLMProvider,
    pub(crate) api_key: String,
    pub(crate) endpoint: String,
    pub(crate) default_model: String,
    pub(crate) llm_type: LLMType,
}

impl LLMClient {
    pub fn new(
        provider: LLMProvider,
        api_key: impl Into<String>,
        endpoint: impl Into<String>,
        default_model: impl Into<String>,
        llm_type: LLMType,
    ) -> Self {
        Self {
            provider,
            api_key: api_key.into(),
            endpoint: endpoint.into(),
            default_model: default_model.into(),
            llm_type,
        }
    }

    pub fn provider(&self) -> LLMProvider {
        self.provider
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    pub fn llm_type(&self) -> LLMType {
        self.llm_type
    }
}


pub type EmbeddingFn = Arc<
    dyn Fn(String) -> Pin<Box<dyn Future<Output = Vec<f32>> + Send + 'static>> + Send + Sync,
>;

pub type ChatFn = Arc<
    dyn Fn(Vec<LLMMessage>) -> Pin<Box<dyn Future<Output = LLMMessage> + Send + 'static>>
        + Send
        + Sync,
>;
