use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use base64::engine::general_purpose::STANDARD as BASE64_ENGINE;
use base64::Engine;
use tokio::fs;
use tokio::fs::try_exists;

use crate::constants::{INPUT_DIR_NAME, OUTPUT_DIR_NAME, PROMPTS_DIR_NAME};
use crate::models::StoredImage;

use std::io;
use std::process::Command;

pub async fn ensure_output_dir() -> Result<PathBuf, String> {
    ensure_library_dir(OUTPUT_DIR_NAME).await
}

pub async fn ensure_input_dir() -> Result<PathBuf, String> {
    ensure_library_dir(INPUT_DIR_NAME).await
}

pub async fn ensure_unique_file_name(dir: &Path, original: &str) -> Result<String, String> {
    if !try_exists(dir.join(original))
        .await
        .map_err(|err| format!("Failed to verify file existence: {}", err))?
    {
        return Ok(original.to_string());
    }

    let original_path = Path::new(original);
    let stem = original_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("image");
    let extension = original_path.extension().and_then(|ext| ext.to_str());

    let mut counter = 1;
    loop {
        let candidate = match extension {
            Some(ext) => format!("{stem}-{counter}.{ext}"),
            None => format!("{stem}-{counter}"),
        };

        if !try_exists(dir.join(&candidate))
            .await
            .map_err(|err| format!("Failed to verify file existence: {}", err))?
        {
            return Ok(candidate);
        }

        counter += 1;
    }
}

pub fn resolve_mime_type(candidate: Option<String>, path: &Path) -> String {
    if let Some(value) = candidate {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream")
        .to_string()
}

pub fn sanitize_file_name(file_name: &str) -> Option<String> {
    let trimmed = file_name.trim();
    if trimmed.is_empty()
        || trimmed.contains(['/', '\\'])
        || trimmed.contains("..")
        || trimmed.contains('\0')
    {
        return None;
    }

    Some(trimmed.to_string())
}

pub async fn read_prompt_file(file_name: &str) -> Result<String, String> {
    let dir = ensure_library_dir(PROMPTS_DIR_NAME).await?;
    let path = dir.join(file_name);

    if !try_exists(&path)
        .await
        .map_err(|err| format!("Failed to check prompt file '{}': {}", path.display(), err))?
    {
        fs::write(&path, "")
            .await
            .map_err(|err| format!("Unable to create prompt file '{}': {}", path.display(), err))?;
        return Ok(String::new());
    }

    fs::read_to_string(&path)
        .await
        .map_err(|err| format!("Unable to read prompt file '{}': {}", path.display(), err))
}

pub async fn write_prompt_file(file_name: &str, contents: &str) -> Result<(), String> {
    let dir = ensure_library_dir(PROMPTS_DIR_NAME).await?;
    let path = dir.join(file_name);

    fs::write(&path, contents)
        .await
        .map_err(|err| format!("Unable to write prompt file '{}': {}", path.display(), err))
}

pub async fn collect_directory_images(dir: &Path) -> Result<Vec<StoredImage>, String> {
    let mut images_with_timestamp: Vec<(StoredImage, u128)> = Vec::new();

    let mut entries = fs::read_dir(dir)
        .await
        .map_err(|err| format!("Unable to read directory '{}': {}", dir.display(), err))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|err| format!("Failed to iterate directory '{}': {}", dir.display(), err))?
    {
        let metadata = entry
            .metadata()
            .await
            .map_err(|err| format!("Failed to read metadata: {}", err))?;

        if !metadata.is_file() {
            continue;
        }

        let path = entry.path();
        let file_name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => continue,
        };

        let guessed_mime = resolve_mime_type(None, &path);
        if !guessed_mime.starts_with("image/") {
            continue;
        }

        let modified_time = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);

        match build_stored_image(&path, metadata.len(), Some(guessed_mime)).await {
            Ok(mut image) => {
                image.id = file_name.clone();
                image.name = file_name;
                images_with_timestamp.push((image, modified_time));
            }
            Err(err) => return Err(err),
        }
    }

    images_with_timestamp.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(images_with_timestamp
        .into_iter()
        .map(|(image, _)| image)
        .collect())
}

pub async fn delete_from_directory(ids: Vec<String>, dir: PathBuf) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }

    for file_id in ids {
        if !is_safe_file_name(&file_id) {
            continue;
        }

        let file_path = dir.join(&file_id);
        if try_exists(&file_path)
            .await
            .map_err(|err| format!("Failed to check file '{}': {}", file_id, err))?
        {
            fs::remove_file(&file_path)
                .await
                .map_err(|err| format!("Failed to delete file '{}': {}", file_id, err))?;
        }
    }

    Ok(())
}

pub async fn build_stored_image(
    path: &Path,
    size: u64,
    provided_mime: Option<String>,
) -> Result<StoredImage, String> {
    let bytes = fs::read(path)
        .await
        .map_err(|err| format!("Unable to read file '{}': {}", path.display(), err))?;
    let mime_type = resolve_mime_type(provided_mime, path);
    let base64 = BASE64_ENGINE.encode(bytes);

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Invalid UTF-8 in file name.".to_string())?;

    Ok(StoredImage {
        id: file_name.to_string(),
        name: file_name.to_string(),
        size,
        mime_type,
        base64,
    })
}

pub fn default_extension_for_mime(mime_type: &str) -> Option<String> {
    let mime = mime_type.trim().to_lowercase();
    let ext = match mime.as_str() {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        "image/bmp" => Some("bmp"),
        "image/tiff" => Some("tiff"),
        _ => None,
    };

    if let Some(value) = ext {
        return Some(value.to_string());
    }

    mime.split('/').nth(1).map(|value| value.to_string())
}

async fn ensure_library_dir(dir_name: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(dir_name);
    if !try_exists(&path)
        .await
        .map_err(|err| format!("Failed to check directory '{}': {}", path.display(), err))?
    {
        fs::create_dir_all(&path)
            .await
            .map_err(|err| format!("Unable to create directory '{}': {}", path.display(), err))?;
    }
    Ok(path)
}

fn is_safe_file_name(file_name: &str) -> bool {
    !file_name.is_empty()
        && !file_name.contains(['/', '\\'])
        && !file_name.contains("..")
        && !file_name.contains('\0')
}

pub fn do_open_dir(path: &str) -> io::Result<()> {
    let path = Path::new(path);

    if !path.is_dir() {
        eprintln!(
            "Error: '{}' is not a directory or does not exist.",
            path.display()
        );
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(path).spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(path).spawn()?;
    }

    Ok(())
}
