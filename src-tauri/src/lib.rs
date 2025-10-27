mod commands;
mod constants;
mod fs_utils;
mod models;

pub use commands::generate::{generate_image, list_generation_logs};
pub use commands::library::{
    delete_images, delete_output_images, get_output_dir_path, list_images, list_output_images,
    open_dir, upload_images,
};
pub use commands::prompts::{
    list_prompt_templates, load_prompts, remove_prompts_by_id, save_prompts,
};

pub use constants::{
    DEFAULT_GEMINI_ENDPOINT, DEFAULT_IMAGE_MIME, DEFAULT_IMAGE_MODEL, INPUT_DIR_NAME,
    OUTPUT_DIR_NAME, PROMPTS_DIR_NAME, SYSTEM_PROMPT_FILE, USER_PROMPT_FILE,
};

pub use models::{
    GenerateImageRequest, GeneratedImage, GeneratedImageResponsePayload, ReferenceImagePayload,
    SavePromptsPayload, StoredImage, UploadImagePayload,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            generate_image,
            list_images,
            list_output_images,
            upload_images,
            delete_images,
            delete_output_images,
            get_output_dir_path,
            open_dir,
            list_prompt_templates,
            load_prompts,
            save_prompts,
            remove_prompts_by_id,
            list_generation_logs
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
