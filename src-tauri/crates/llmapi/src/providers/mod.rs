mod anthropic;
pub mod gemini;
mod openai;

use crate::types::{ChatFn, LLMClient, LLMProvider};

pub use anthropic::{chat as anthropic_chat, embedding as anthropic_embedding};
pub use gemini::{
    chat as gemini_chat, convert_body_parts_gemini, embedding as gemini_embedding,
    send_generate_request,
};
pub use openai::{chat as openai_chat, embedding as openai_embedding};

pub async fn get_llm_chat(client: LLMClient) -> ChatFn {
    match client.provider() {
        LLMProvider::OpenAI => openai_chat(client).await,
        LLMProvider::Anthropic => anthropic_chat(client).await,
        LLMProvider::Gemini => gemini_chat(client).await,
    }
}

pub async fn get_llm_embedding(client: LLMClient) -> Box<dyn Fn(Vec<String>) -> Vec<Vec<f32>>> {
    match client.provider() {
        LLMProvider::Gemini => gemini_embedding(client).await,
        LLMProvider::OpenAI => openai_embedding(client).await,
        LLMProvider::Anthropic => Box::new(|_| Vec::new()),
    }
}
