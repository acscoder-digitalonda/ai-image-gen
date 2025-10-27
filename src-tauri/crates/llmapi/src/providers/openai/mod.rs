mod api;
mod models;

pub use api::chat;

use crate::types::LLMClient;

use api::openai_embed_texts;

pub async fn embedding(client: LLMClient) -> Box<dyn Fn(Vec<String>) -> Vec<Vec<f32>>> {
    Box::new(move |input: Vec<String>| -> Vec<Vec<f32>> {
        if input.is_empty() {
            return Vec::new();
        }

        let client = client.clone();

        let handle = std::thread::spawn(move || match tokio::runtime::Runtime::new() {
            Ok(runtime) => match runtime.block_on(openai_embed_texts(&client, &input)) {
                Ok(values) => values,
                Err(err) => {
                    eprintln!("OpenAI embedding error: {err}");
                    Vec::new()
                }
            },
            Err(err) => {
                eprintln!("Failed to create Tokio runtime for OpenAI embeddings: {err}");
                Vec::new()
            }
        });

        match handle.join() {
            Ok(values) => values,
            Err(_) => {
                eprintln!("OpenAI embedding thread panicked");
                Vec::new()
            }
        }
    })
}
