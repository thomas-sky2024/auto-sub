use crate::{
    cache, error::{AutoSubError, Result},
    ffmpeg, job_manager::JobManager, post_process, subtitle, thermal, validator, whisper,
};
use log::info;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

/// Frontend-facing progress event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressPayload {
    pub stage: String,
    pub percent: f32,
    pub segment_count: usize,
}

/// Options for a transcription job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineOptions {
    pub video_path: String,
    pub language: String,
    pub model: String,
    pub performance_mode: thermal::PerformanceMode,
}

/// Output of a completed pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineResult {
    pub segments: Vec<subtitle::Segment>,
    pub srt_content: String,
    pub txt_content: String,
    pub duration_secs: f32,
    pub from_cache: bool,
}

/// Emit a progress event to the Tauri frontend.
fn emit_progress(app: &AppHandle, stage: &str, percent: f32, segment_count: usize) {
    let _ = app.emit(
        "pipeline-progress",
        ProgressPayload {
            stage: stage.to_string(),
            percent,
            segment_count,
        },
    );
}

/// Run the full 5-stage pipeline.
pub async fn run(
    app: AppHandle,
    opts: PipelineOptions,
    job_mgr: Arc<JobManager>,
) -> Result<PipelineResult> {
    let video_path = &opts.video_path;
    let model_name = &opts.model;
    let lang = &opts.language;

    // ── Stage 0: Check cache ──────────────────────────────────────────────────
    emit_progress(&app, "Checking cache", 2.0, 0);
    if let Ok(Some(cached_srt)) = cache::check_cache(video_path, model_name, lang) {
        let srt_content = tokio::fs::read_to_string(&cached_srt).await.map_err(|e| {
            AutoSubError::Cache(format!("Failed to read cached SRT: {}", e))
        })?;
        info!("pipeline: serving from cache");
        emit_progress(&app, "Done (from cache)", 100.0, 0);
        let segments = parse_srt_to_segments(&srt_content);
        let txt = subtitle::to_txt(&segments);
        let dur = ffmpeg::get_video_duration(&resolve_bin("ffmpeg"), video_path)
            .await
            .unwrap_or(0.0);
        return Ok(PipelineResult {
            segments,
            srt_content,
            txt_content: txt,
            duration_secs: dur,
            from_cache: true,
        });
    }

    // ── Stage 1: Audio extraction ─────────────────────────────────────────────
    emit_progress(&app, "Extracting audio", 5.0, 0);
    job_mgr.update_progress("Extracting audio", 5.0);

    let ffmpeg_bin = resolve_bin("ffmpeg");
    let whisper_bin = resolve_bin("whisper-main");
    let model_path = resolve_model(model_name);

    // Get video duration for progress calculation
    let duration_secs = ffmpeg::get_video_duration(&ffmpeg_bin, video_path)
        .await
        .unwrap_or(0.0);

    // Set up audio output path
    let cache_dir = cache::cache_dir(video_path)?;
    tokio::fs::create_dir_all(&cache_dir).await.map_err(|e| {
        AutoSubError::Cache(format!("Failed to create cache dir: {}", e))
    })?;
    let audio_path = cache_dir.join("audio.wav").to_string_lossy().to_string();

    // Progress forwarding from ffmpeg
    let (ffmpeg_tx, mut ffmpeg_rx) = mpsc::channel::<ffmpeg::FfmpegProgress>(32);
    let app_clone = app.clone();
    let jm_clone = job_mgr.clone();
    tokio::spawn(async move {
        while let Some(p) = ffmpeg_rx.recv().await {
            let scaled = 5.0 + p.percent * 0.30; // 5% → 35%
            emit_progress(&app_clone, "Extracting audio", scaled, 0);
            jm_clone.update_progress("Extracting audio", scaled);
        }
    });

    ffmpeg::extract_audio(
        &ffmpeg_bin,
        video_path,
        &audio_path,
        duration_secs,
        Some(ffmpeg_tx),
    )
    .await?;

    emit_progress(&app, "Transcribing", 35.0, 0);
    job_mgr.update_progress("Transcribing", 35.0);

    // ── Stage 2: Whisper transcription ────────────────────────────────────────
    let threads = thermal::recommended_threads(opts.performance_mode);
    let output_dir = cache_dir.to_string_lossy().to_string();

    let (whisper_tx, mut whisper_rx) = mpsc::channel::<whisper::WhisperProgress>(32);
    let app_clone = app.clone();
    let jm_clone = job_mgr.clone();
    tokio::spawn(async move {
        while let Some(p) = whisper_rx.recv().await {
            let scaled = 35.0 + p.percent * 0.45; // 35% → 80%
            emit_progress(&app_clone, "Transcribing", scaled, 0);
            jm_clone.update_progress("Transcribing", scaled);
        }
    });

    let raw_segments = whisper::transcribe(
        &whisper_bin,
        &model_path,
        &audio_path,
        &output_dir,
        lang,
        threads,
        Some(whisper_tx),
    )
    .await?;

    emit_progress(&app, "Validating", 80.0, raw_segments.len());
    job_mgr.update_progress("Validating", 80.0);

    // ── Stage 3: Validation ───────────────────────────────────────────────────
    let validated = validator::validate(raw_segments);

    emit_progress(&app, "Post-processing", 85.0, validated.len());
    job_mgr.update_progress("Post-processing", 85.0);

    // ── Stage 4: Post-processing ──────────────────────────────────────────────
    let processed = post_process::process(validated);

    emit_progress(&app, "Exporting", 95.0, processed.len());
    job_mgr.update_progress("Exporting", 95.0);

    // ── Stage 5: Export ───────────────────────────────────────────────────────
    let srt_content = subtitle::to_srt(&processed);
    let txt_content = subtitle::to_txt(&processed);

    // Save to cache
    cache::save_final(video_path, &srt_content, model_name, lang, duration_secs)?;

    emit_progress(&app, "Done", 100.0, processed.len());
    job_mgr.complete();

    Ok(PipelineResult {
        segments: processed,
        srt_content,
        txt_content,
        duration_secs,
        from_cache: false,
    })
}

/// Resolve a bundled binary path.
fn resolve_bin(name: &str) -> String {
    // Try to find in same directory as this executable
    if let Ok(mut exe) = std::env::current_exe() {
        exe.pop(); // remove executable name
        let bin = exe.join(name);
        if bin.exists() {
            return bin.to_string_lossy().to_string();
        }
    }
    // Fall back to system PATH
    name.to_string()
}

/// Resolve model path from model name.
fn resolve_model(model_name: &str) -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let model_file = format!("ggml-{}.bin", model_name);
    home.join(".autosub")
        .join("models")
        .join(&model_file)
        .to_string_lossy()
        .to_string()
}

/// Minimal SRT parser for cache-hit path (parse back to segments).
fn parse_srt_to_segments(srt: &str) -> Vec<subtitle::Segment> {
    let mut segments = Vec::new();
    let blocks: Vec<&str> = srt.trim().split("\n\n").collect();

    for block in blocks {
        let lines: Vec<&str> = block.lines().collect();
        if lines.len() < 3 {
            continue;
        }

        // Line 0: index, Line 1: timestamps, Line 2+: text
        let ts_line = lines[1];
        let parts: Vec<&str> = ts_line.split(" --> ").collect();
        if parts.len() != 2 {
            continue;
        }

        let start = parse_srt_time(parts[0].trim());
        let end = parse_srt_time(parts[1].trim());
        let text = lines[2..].join("\n");

        segments.push(subtitle::Segment { start, end, text });
    }

    segments
}

fn parse_srt_time(ts: &str) -> f32 {
    // Format: HH:MM:SS,mmm
    let (hms, ms_part) = ts.split_once(',').unwrap_or((ts, "0"));
    let parts: Vec<f32> = hms.split(':').filter_map(|p| p.parse().ok()).collect();
    let ms: f32 = ms_part.parse().unwrap_or(0.0) / 1000.0;
    if parts.len() == 3 {
        parts[0] * 3600.0 + parts[1] * 60.0 + parts[2] + ms
    } else {
        0.0
    }
}
