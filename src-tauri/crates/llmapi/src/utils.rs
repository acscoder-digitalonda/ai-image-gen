use anyhow::{Context, Result};
use base64::Engine as _;
use reqwest::Client;
use std::fs;
use std::path::{Path, PathBuf};

pub fn detect_mime_type<P: AsRef<Path>>(path: P) -> String {
    mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("image/jpeg")
        .to_string()
}
pub async fn download_image(url: &str) -> Result<Vec<u8>> {
    let client = Client::new();

    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to send request to {}", url))?
        .error_for_status()
        .with_context(|| format!("Non-success HTTP status from {}", url))?;

    let bytes = resp
        .bytes()
        .await
        .context("Failed to read response bytes")?;

    Ok(bytes.to_vec())
}
pub fn is_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

pub fn encode_image_to_base64(img_path: &str) -> Result<String> {
    if is_http_url(img_path) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let bytes = rt.block_on(download_image(img_path))?;
        let encoded = encode_byte_to_base64(bytes);
        return Ok(encoded);
    } else {
        let bytes = fs::read(img_path)
            .with_context(|| format!("Failed to read image file: {}", img_path))?;
        let encoded = encode_byte_to_base64(bytes);
        Ok(encoded)
    }
}
pub fn encode_byte_to_base64(bytes: Vec<u8>) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    encoded
}
pub fn current_timestamp_millis() -> u64 {
    let now = std::time::SystemTime::now();
    let duration_since_epoch = now
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards");
    duration_since_epoch.as_millis() as u64
}

pub fn save_images_to_output_dir(images: &Vec<Vec<u8>>, output_dir: &Path) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create output directory {:?}", output_dir))?;

    let mut saved_paths = Vec::new();

    for (index, img_bytes) in images.iter().enumerate() {
        let img_path = output_dir.join(format!("image_{:03}.png", index));
        fs::write(&img_path, img_bytes)
            .with_context(|| format!("Failed to write image file {:?}", img_path))?;

        println!("✅ Saved image to {:?}", img_path);
        saved_paths.push(img_path);
    }

    Ok(saved_paths)
}
