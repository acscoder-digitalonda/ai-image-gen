mod api;
pub mod models;
pub use api::{
    convert_body_parts_gemini, gemini_embed_texts, response_to_base64_images,
    response_to_text_data, send_generate_request,
};

use crate::types::{ChatFn, LLMClient, LLMMessage, LLMMessageType};
use std::sync::Arc;

pub async fn chat(client: LLMClient) -> ChatFn {
    Arc::new(move |messages: Vec<LLMMessage>| {
        let client = client.clone();
        Box::pin(async move {
            if let Ok(response) = send_generate_request(&client, messages).await {
                let mut data_response: Vec<LLMMessageType> = vec![];
                let images = response_to_base64_images(&response);
                if !images.is_empty() {
                    for image in images {
                        data_response.push(LLMMessageType::image_b64(image));
                    }
                }
                data_response.push(LLMMessageType::text(
                    response_to_text_data(&response).unwrap_or("".to_owned()),
                ));
                return LLMMessage::new(response.response_id.clone(), "AI", data_response);
            } else {
                return LLMMessage::new(
                    None,
                    "System",
                    vec![LLMMessageType::text("Failed to get response from Gemini")],
                );
            }
        })
    })
}

pub async fn embedding(client: LLMClient) -> Box<dyn Fn(Vec<String>) -> Vec<Vec<f32>>> {
    Box::new(move |input: Vec<String>| -> Vec<Vec<f32>> {
        let client = client.clone();

        let handle = std::thread::spawn(move || match tokio::runtime::Runtime::new() {
            Ok(runtime) => match runtime.block_on(gemini_embed_texts(&client, &input)) {
                Ok(values) => values,
                Err(err) => {
                    eprintln!("Gemini embedding error: {err}");
                    Vec::new()
                }
            },
            Err(err) => {
                eprintln!("Failed to create Tokio runtime for Gemini embeddings: {err}");
                Vec::new()
            }
        });

        match handle.join() {
            Ok(values) => values,
            Err(_) => {
                eprintln!("Gemini embedding thread panicked");
                Vec::new()
            }
        }
    })
}
