use crate::fs_utils::ensure_input_dir;
use crate::models::SavePromptsPayload;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;
use tokio::fs::try_exists;

const PROMPT_TEMPLATES_FILE: &str = "prompts.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplates {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub date_created: u64,
}

impl PromptTemplates {
    pub fn new(
        id: Option<String>,
        name: String,
        system_prompt: String,
        user_prompt: String,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);

        Self {
            id: id.unwrap_or_else(|| now.to_string()),
            name,
            system_prompt,
            user_prompt,
            date_created: now,
        }
    }
}

#[tauri::command]
pub async fn load_prompts(prompt_name: String) -> Result<PromptTemplates, String> {
    let templates = read_prompt_templates().await?;

    templates
        .into_iter()
        .find(|template| template.name == prompt_name)
        .ok_or_else(|| format!("Prompt template '{}' not found.", prompt_name))
}

#[tauri::command]
pub async fn save_prompts(payload: SavePromptsPayload) -> Result<PromptTemplates, String> {
    let SavePromptsPayload {
        id,
        name,
        system_prompt,
        user_prompt,
    } = payload;

    let mut templates = read_prompt_templates().await?;

    if let Some(existing_id) = id.clone() {
        if let Some(index) = templates
            .iter()
            .position(|template| template.id == existing_id)
        {
            let updated = {
                let existing = templates
                    .get_mut(index)
                    .expect("template index resolved from position must exist");
                existing.name = name.clone();
                existing.system_prompt = system_prompt.clone();
                existing.user_prompt = user_prompt.clone();
                existing.clone()
            };

            write_prompt_templates(&templates).await?;
            return Ok(updated);
        }
    }

    if id.is_none() {
        if let Some(index) = templates.iter().position(|template| template.name == name) {
            let updated = {
                let existing = templates
                    .get_mut(index)
                    .expect("template index resolved from position must exist");
                existing.system_prompt = system_prompt.clone();
                existing.user_prompt = user_prompt.clone();
                existing.clone()
            };

            write_prompt_templates(&templates).await?;
            return Ok(updated);
        }
    }

    let mut template = PromptTemplates::new(id, name, system_prompt, user_prompt);

    if templates.iter().any(|existing| existing.id == template.id) {
        template.id = generate_unique_id(&templates);
    }

    templates.push(template.clone());
    write_prompt_templates(&templates).await?;
    Ok(template)
}

#[tauri::command]
pub async fn remove_prompts_by_id(id: String) -> Result<(), String> {
    let mut templates = read_prompt_templates().await?;
    let original_len = templates.len();
    templates.retain(|template| template.id != id);

    if templates.len() == original_len {
        return Err(format!("Prompt template with id '{}' not found.", id));
    }

    write_prompt_templates(&templates).await?;
    Ok(())
}

#[tauri::command]
pub async fn list_prompt_templates() -> Result<Vec<PromptTemplates>, String> {
    read_prompt_templates().await
}

async fn prompt_templates_path() -> Result<PathBuf, String> {
    let dir = ensure_input_dir().await?;
    Ok(dir.join(PROMPT_TEMPLATES_FILE))
}

async fn read_prompt_templates() -> Result<Vec<PromptTemplates>, String> {
    let path = prompt_templates_path().await?;

    if !try_exists(&path)
        .await
        .map_err(|err| format!("Failed to check prompts file '{}': {}", path.display(), err))?
    {
        fs::write(&path, "[]").await.map_err(|err| {
            format!(
                "Unable to create prompts file '{}': {}",
                path.display(),
                err
            )
        })?;
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(&path)
        .await
        .map_err(|err| format!("Unable to read prompts file '{}': {}", path.display(), err))?;

    if contents.trim().is_empty() {
        return Ok(Vec::new());
    }

    serde_json::from_str(&contents)
        .map_err(|err| format!("Unable to parse prompts file '{}': {}", path.display(), err))
}

async fn write_prompt_templates(templates: &[PromptTemplates]) -> Result<(), String> {
    let path = prompt_templates_path().await?;
    let payload = serde_json::to_string_pretty(templates)
        .map_err(|err| format!("Unable to serialise prompt templates: {}", err))?;

    fs::write(&path, payload)
        .await
        .map_err(|err| format!("Unable to write prompts file '{}': {}", path.display(), err))
}

fn generate_unique_id(existing: &[PromptTemplates]) -> String {
    let mut counter = 0u64;
    loop {
        let candidate = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
            + counter;

        let candidate = candidate.to_string();

        if existing.iter().all(|template| template.id != candidate) {
            return candidate;
        }

        counter += 1;
    }
}
