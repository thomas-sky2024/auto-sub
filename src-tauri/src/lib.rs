mod cache;
mod downloader;
mod error;
mod ffmpeg;
mod job_manager;
mod pipeline;
mod model_manager;
mod post_process;
mod subtitle;
mod sensevoice;
mod thermal;
mod utils;
mod validator;

use job_manager::JobManager;
use pipeline::{PipelineOptions, PipelineResult};
use std::sync::Arc;
use tauri::{AppHandle, State};
use tauri_plugin_shell::ShellExt;

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

/// Check if a specific model is ready.
#[tauri::command]
async fn check_model(model_id: String) -> Result<bool, error::AutoSubError> {
    Ok(model_manager::ModelManager::is_ready(&model_id))
}

/// List all models for UI.
#[tauri::command]
async fn list_all_models() -> Result<Vec<model_manager::ModelInfo>, error::AutoSubError> {
    Ok(model_manager::ModelManager::all_model_info())
}

/// List IDs of models that are downloaded and ready.
#[tauri::command]
async fn list_ready_models() -> Result<Vec<String>, error::AutoSubError> {
    Ok(model_manager::ModelManager::list_ready_ids())
}

#[derive(serde::Serialize)]
pub struct EnvironmentAudit {
    pub ffmpeg: bool,
    pub sherpa_onnx: bool,
    pub ytdlp: bool,
    pub vad_ready: bool,
    pub ready_models: Vec<String>,
    pub models_dir: String,
}

/// Audit the system environment for required dependencies.
#[tauri::command]
async fn audit_environment(app: AppHandle) -> Result<EnvironmentAudit, error::AutoSubError> {
    let ffmpeg_ok = app.shell().sidecar("ffmpeg").is_ok();
    let sherpa_ok = app.shell().sidecar("sherpa-onnx").is_ok();
    let ytdlp_ok = app.shell().sidecar("yt-dlp").is_ok();
    let vad_ok = model_manager::ModelManager::vad_ready();
    let ready = model_manager::ModelManager::list_ready_ids();

    Ok(EnvironmentAudit {
        ffmpeg: ffmpeg_ok,
        sherpa_onnx: sherpa_ok,
        ytdlp: ytdlp_ok,
        vad_ready: vad_ok,
        ready_models: ready,
        models_dir: model_manager::ModelManager::get_models_dir(),
    })
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

/// Clear all cached data.
#[tauri::command]
async fn clear_cache() -> Result<(), error::AutoSubError> {
    cache::clear_all_cache()
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
            list_all_models,
            list_ready_models,
            audit_environment,
            export_file,
            clear_cache,
            downloader::download_media,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AutoSub");
}
