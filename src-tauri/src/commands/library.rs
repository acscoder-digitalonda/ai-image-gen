use base64::engine::general_purpose::STANDARD as BASE64_ENGINE;
use base64::Engine;
use tokio::fs;

use crate::fs_utils::{
    collect_directory_images, delete_from_directory, do_open_dir, ensure_input_dir,
    ensure_output_dir, ensure_unique_file_name, resolve_mime_type, sanitize_file_name,
};
use crate::models::{StoredImage, UploadImagePayload};

#[tauri::command]
pub fn open_dir(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Directory path cannot be empty.".into());
    }

    do_open_dir(trimmed).map_err(|err| format!("Failed to open directory '{}': {}", trimmed, err))
}

#[tauri::command]
pub async fn list_images() -> Result<Vec<StoredImage>, String> {
    let input_dir = ensure_input_dir().await?;
    collect_directory_images(&input_dir).await
}

#[tauri::command]
pub async fn upload_images(payloads: Vec<UploadImagePayload>) -> Result<Vec<StoredImage>, String> {
    if payloads.is_empty() {
        return Ok(Vec::new());
    }

    let input_dir = ensure_input_dir().await?;
    let mut stored_images = Vec::new();

    for payload in payloads {
        let UploadImagePayload {
            file_name,
            mime_type,
            data_base64,
        } = payload;

        let sanitized_name = sanitize_file_name(&file_name)
            .ok_or_else(|| format!("Invalid file name supplied: {}", file_name))?;

        let unique_name = ensure_unique_file_name(&input_dir, &sanitized_name).await?;
        let target_path = input_dir.join(&unique_name);

        let trimmed_base64 = data_base64.trim();
        let data = BASE64_ENGINE
            .decode(trimmed_base64)
            .map_err(|err| format!("Failed to decode image '{}': {}", file_name, err))?;

        fs::write(&target_path, &data)
            .await
            .map_err(|err| format!("Unable to write file '{}': {}", unique_name, err))?;

        let resolved_mime = resolve_mime_type(mime_type, &target_path);

        stored_images.push(StoredImage {
            id: unique_name.clone(),
            name: unique_name,
            size: data.len() as u64,
            mime_type: resolved_mime,
            base64: trimmed_base64.to_string(),
        });
    }

    Ok(stored_images)
}

#[tauri::command]
pub async fn delete_images(ids: Vec<String>) -> Result<(), String> {
    delete_from_directory(ids, ensure_input_dir().await?).await
}

#[tauri::command]
pub async fn list_output_images() -> Result<Vec<StoredImage>, String> {
    let output_dir = ensure_output_dir().await?;
    collect_directory_images(&output_dir).await
}

#[tauri::command]
pub async fn delete_output_images(ids: Vec<String>) -> Result<(), String> {
    delete_from_directory(ids, ensure_output_dir().await?).await
}

#[tauri::command]
pub async fn get_output_dir_path() -> Result<String, String> {
    let dir = ensure_output_dir().await?;
    dir.to_str()
        .map(|value| value.to_string())
        .ok_or_else(|| "Output directory path is not valid UTF-8.".to_string())
}
