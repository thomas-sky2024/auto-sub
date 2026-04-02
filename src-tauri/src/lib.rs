mod cache;
mod downloader;
mod error;
mod ffmpeg;
mod job_manager;
mod pipeline;
mod model_manager;
mod post_process;
mod subtitle;
mod thermal;
mod utils;
mod validator;
mod whisper;

use job_manager::JobManager;
use pipeline::{PipelineOptions, PipelineResult};
use std::sync::Arc;
use tauri::{AppHandle, State};

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

/// Check if a model file exists and is valid.
#[tauri::command]
async fn check_model(model_name: String) -> Result<bool, error::AutoSubError> {
    Ok(model_manager::ModelManager::verify_model(&model_name))
}

/// List available verified models.
#[tauri::command]
async fn list_models() -> Result<Vec<String>, error::AutoSubError> {
    Ok(model_manager::ModelManager::list_models())
}

#[derive(serde::Serialize)]
pub struct EnvironmentAudit {
    pub ffmpeg: bool,
    pub whisper: bool,
    pub ytdlp: bool,
    pub models_dir: String,
}

/// Audit the system environment for required dependencies.
#[tauri::command]
async fn audit_environment() -> Result<EnvironmentAudit, error::AutoSubError> {
    let ffmpeg_path = utils::resolve_bin("ffmpeg");
    let whisper_path = utils::resolve_bin("whisper-main");
    let ytdlp_path = utils::resolve_bin("yt-dlp");

    Ok(EnvironmentAudit {
        ffmpeg: std::path::Path::new(&ffmpeg_path).exists(),
        whisper: std::path::Path::new(&whisper_path).exists(),
        ytdlp: std::path::Path::new(&ytdlp_path).exists(),
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
            audit_environment,
            export_file,
            downloader::download_media,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AutoSub");
}
