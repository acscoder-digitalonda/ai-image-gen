# llmapi

`llmapi` is a small Rust library that normalizes how this workspace talks to large language model APIs. It wraps OpenAI, Anthropic, and Google Gemini chat endpoints behind a shared interface, handles multimodal messages (text plus images), and exposes embedding helpers where the remote API supports them.

## Features
- Unified chat invocation via `get_llm_chat`, returning a future-friendly closure you can reuse across the app.
- Embedding helpers for OpenAI and Gemini (with graceful fallbacks elsewhere).
- Multimodal message conversion that accepts text, local image paths, or pre-encoded base64 payloads.
- Utility helpers for MIME detection, base64 encoding/decoding, timestamping, and saving generated images.
- Thin, dependency-light implementation built on `reqwest`, `tokio`, and `serde`.

## Getting Started
Add the crate to a Rust workspace or depend on it directly:

```toml
[dependencies]
llmapi = { path = "src-tauri/crates/llmapi" }
anyhow = "1"
tokio = { version = "1", features = ["full"] }
```

### Configure a client

```rust
use anyhow::Result;
use llmapi::{
    get_llm_chat,
    get_llm_embedding,
    LLMClient,
    LLMMessage,
    LLMMessageType,
    LLMProvider,
    LLMType,
};

#[tokio::main]
async fn main() -> Result<()> {
    let client = LLMClient::new(
        LLMProvider::OpenAI,
        std::env::var("OPENAI_API_KEY")?,
        "https://api.openai.com/v1",
        "gpt-4o-mini",
        LLMType::Chat,
    );

    let user_message = LLMMessage::new(
        None,
        "user",
        vec![LLMMessageType::text("Generate a haiku about Rust.")],
    );

    let chat = get_llm_chat(client.clone()).await;
    let reply = chat(vec![user_message]).await;
    println!("{reply:?}");

    Ok(())
}
```

For embeddings:

```rust
let client = LLMClient::new(
    LLMProvider::Gemini,
    std::env::var("GEMINI_API_KEY")?,
    "https://generativelanguage.googleapis.com/v1beta",
    "models/text-embedding-004",
    LLMType::Embedding,
);

let embed = get_llm_embedding(client).await;
let vectors = embed(vec![
    "Rust is a systems programming language.".into(),
    "Ferris is pretty great.".into(),
]);
println!("Got {} vectors.", vectors.len());
```

## Provider Notes

- **OpenAI** (`reqwest` + bearer auth)  
  Supports text or image parts per message. Chat responses are normalized into `LLMMessageType::TEXT` or `::IMAGE`. Embedding requests hit `/embeddings`. Set the endpoint (usually `https://api.openai.com/v1`) and pick a compatible model ID.

- **Anthropic** (Claude Messages API)  
  Chat requests merge all system prompts into a single `system` string and send user/assistant turns with optional image parts. The `embedding` helper is currently a stub that returns an empty placeholder vector; use `LLMProvider::OpenAI` or `LLMProvider::Gemini` if you need real embeddings.

- **Gemini** (Google Generative Language API)  
  Chat responses may contain inline base64 image data as well as text. The crate exposes `response_to_base64_images`, `response_to_image_data`, and `response_to_text_data` helpers so callers can decide how to render outputs. Embedding helpers automatically switch between the single-item `:embedContent` endpoint and the batch `:batchEmbedContents` variant.

## Working With Messages
- `LLMMessage::new(None, "user", parts)` assigns a timestamp-based ID if you omit one.
- `LLMMessageType::image(path)` accepts local file paths or HTTP URLs; files are read (or downloaded) and converted to base64 automatically.
- `convert_body_parts_gemini` and `convert_messages_to_gemini_contents` live in the Gemini module but illustrate the expected intermediate shape if you need to debug payloads.

### Reproducing the test harness locally
The integration-style tests under `tests/test.rs` show how to exercise each provider end-to-end. To run them:

1. Copy `tests/test.rs` to your own crate or keep the paths as-is if you depend on this crate locally.
2. Create a `test_data` directory at the repository root and add any seed assets (for example, `test_data/inp.png` for the Gemini multimodal test).
3. Replace the placeholder API keys with real credentials or load them from the environment:
   ```rust
   let api_key = std::env::var("GEMINI_API_KEY")?;
   ```
4. Execute `cargo test chat_gemini -- --nocapture` (or any specific test) to stream the responses. The helpers `ensure_test_data_dir`, `save_response_json`, and `save_images_to_test_data` will persist responses for inspection.

Each test demonstrates:
- Setting the correct API endpoint and model for OpenAI, Anthropic, and Gemini.
- Building message payloads with either pure text or text + images.
- Fetching embeddings that align with the number of requested inputs (or empty vectors on failure).

## Utilities
- `utils::detect_mime_type` identifies image MIME types with `mime_guess`.
- `utils::encode_image_to_base64` reads from disk or fetches over HTTP before encoding.
- `utils::save_images_to_output_dir` writes generated image bytes to disk (`image_000.png`, etc.).

## Development
- Run the unit tests (currently lightweight) with `cargo test`.
- The helper `log_request_payload` (Gemini) is available to persist the last API payload under `test_data/last_gemini_request.json`; uncomment explicit calls when you need to debug payloads locally.

## Environment
The crate expects you to supply fully qualified endpoints and API keys when constructing an `LLMClient`. Typical defaults:

- OpenAI: `https://api.openai.com/v1`
- Anthropic: `https://api.anthropic.com/v1`
- Gemini: `https://generativelanguage.googleapis.com/v1beta`

Do **not** hardcode keys; read them from environment variables or a secrets manager in your application layer.

## License
This crate follows the license of the parent workspace. See the repository root for details.
