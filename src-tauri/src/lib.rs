mod cache;
mod error;
mod ffmpeg;
mod job_manager;
mod pipeline;
mod post_process;
mod subtitle;
mod thermal;
mod validator;
mod whisper;

use job_manager::JobManager;
use pipeline::{PipelineOptions, PipelineResult};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

/// Shared app state — thread-safe job manager.
pub struct AppState {
    pub job_manager: Arc<JobManager>,
}

// ── Tauri Commands ────────────────────────────────────────────────────────────

/// Start the transcription pipeline.
#[tauri::command]
async fn start_pipeline(
    app: AppHandle,
    state: State<'_, AppState>,
    opts: PipelineOptions,
) -> Result<PipelineResult, error::AutoSubError> {
    let jm = state.job_manager.clone();
    jm.reset();

    // Run in async task (never blocks the main thread)
    pipeline::run(app, opts, jm).await
}

/// Cancel the current job.
#[tauri::command]
async fn cancel_job(state: State<'_, AppState>) -> Result<(), error::AutoSubError> {
    state.job_manager.cancel();
    Ok(())
}

/// Get current job state.
#[tauri::command]
async fn get_job_state(
    state: State<'_, AppState>,
) -> Result<job_manager::JobState, error::AutoSubError> {
    Ok(state.job_manager.state())
}

/// Check if a model file exists.
#[tauri::command]
async fn check_model(model_name: String) -> Result<bool, error::AutoSubError> {
    let home = dirs::home_dir().unwrap_or_default();
    let model_path = home
        .join(".autosub")
        .join("models")
        .join(format!("ggml-{}.bin", model_name));
    Ok(model_path.exists())
}

/// List available models.
#[tauri::command]
async fn list_models() -> Result<Vec<String>, error::AutoSubError> {
    let home = dirs::home_dir().unwrap_or_default();
    let models_dir = home.join(".autosub").join("models");
    let mut models = Vec::new();

    if let Ok(entries) = std::fs::read_dir(models_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("ggml-") && name.ends_with(".bin") {
                let model_name = name
                    .trim_start_matches("ggml-")
                    .trim_end_matches(".bin")
                    .to_string();
                models.push(model_name);
            }
        }
    }

    Ok(models)
}

/// Export SRT content to a file.
#[tauri::command]
async fn export_file(
    path: String,
    content: String,
) -> Result<(), error::AutoSubError> {
    tokio::fs::write(&path, content).await.map_err(|e| {
        error::AutoSubError::Export(format!("Failed to write file: {}", e))
    })
}

// ── App Entry Point ───────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize structured logging
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            job_manager: Arc::new(JobManager::new()),
        })
        .invoke_handler(tauri::generate_handler![
            start_pipeline,
            cancel_job,
            get_job_state,
            check_model,
            list_models,
            export_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AutoSub");
}
