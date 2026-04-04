use crate::{
    cache, error::{AutoSubError, Result},
    ffmpeg, job_manager::JobManager, model_manager::ModelManager, post_process, subtitle,
    thermal, validator, sensevoice,
};
use tauri_plugin_shell::ShellExt;
use log::{info, warn};
use serde::{Deserialize, Serialize};
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
    pub model_id: String,
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
    let model_id = &opts.model_id;
    let lang = &opts.language;

    // Resolve model config
    let model_config = ModelManager::get_config(model_id).ok_or_else(|| {
        AutoSubError::WhisperDecode(format!("Unknown model ID: {}", model_id))
    })?;

    // Validate model files exist
    if !ModelManager::is_ready(model_id) {
        return Err(AutoSubError::WhisperDecode(format!(
            "Model '{}' is not ready (VAD or model files missing). Please run setup-models.sh",
            model_id
        )));
    }

    // ── Stage 0: Check cache ──────────────────────────────────────────────────
    emit_progress(&app, "Checking cache", 2.0, 0);
    if let Ok(Some(cached_srt)) = cache::check_cache(video_path, model_id, lang) {
        let srt_content = tokio::fs::read_to_string(&cached_srt).await.map_err(|e| {
            AutoSubError::Cache(format!("Failed to read cached SRT: {}", e))
        })?;
        info!("pipeline: serving from cache");
        emit_progress(&app, "Done (from cache)", 100.0, 0);
        let segments = parse_srt_to_segments(&srt_content);
        let txt = subtitle::to_txt(&segments);
        let ffmpeg_sidecar = app.shell().sidecar("ffmpeg").ok();
        let dur = if let Some(sidecar) = ffmpeg_sidecar {
            ffmpeg::get_video_duration(sidecar, video_path).await.unwrap_or(0.0)
        } else {
            0.0
        };
        return Ok(PipelineResult {
            segments,
            srt_content,
            txt_content: txt,
            duration_secs: dur,
            from_cache: true,
        });
    }

    let ffprobe_sidecar = app.shell().sidecar("ffprobe").ok();
    let duration_secs = if let Some(ffp) = ffprobe_sidecar {
        ffmpeg::get_video_duration(ffp, video_path).await.unwrap_or(0.0)
    } else {
        warn!("ffprobe sidecar not found, duration might be inaccurate");
        0.0
    };

    // ── Stage 1: Audio extraction ─────────────────────────────────────────────
    emit_progress(&app, "Extracting audio", 5.0, 0);
    job_mgr.update_progress("Extracting audio", 5.0);
    cache::update_state(video_path, model_id, lang, duration_secs, cache::PipelineState::Extracting)?;

    // Set up audio output path
    let cache_dir = cache::cache_dir(video_path)?;
    tokio::fs::create_dir_all(&cache_dir).await.map_err(|e| {
        AutoSubError::Cache(format!("Failed to create cache dir: {}", e))
    })?;
    let audio_path = cache_dir.join("audio.wav").to_string_lossy().to_string();


    // Run ffmpeg with retry
    crate::utils::retry(|| async {
        let (tx, mut rx) = mpsc::channel::<ffmpeg::FfmpegProgress>(32);
        let app_clone = app.clone();
        let jm_clone = job_mgr.clone();
        tokio::spawn(async move {
            while let Some(p) = rx.recv().await {
                let scaled = 5.0 + p.percent * 0.30; // 5% → 35%
                emit_progress(&app_clone, "Extracting audio", scaled, 0);
                jm_clone.update_progress("Extracting audio", scaled);
            }
        });

        let sidecar = app.shell().sidecar("ffmpeg").map_err(|e| 
            AutoSubError::SidecarNotFound(format!("ffmpeg sidecar not found: {}", e))
        )?;

        ffmpeg::extract_audio(
            sidecar,
            video_path,
            &audio_path,
            duration_secs,
            Some(tx),
        ).await
    }, 2).await?;

    cache::update_state(video_path, model_id, lang, duration_secs, cache::PipelineState::Extracted)?;

    // ── STAGE 1.5: Vocal Separation (Removed in favor of SenseVoice) ─────────
    let final_audio = audio_path.clone();

    emit_progress(&app, "Transcribing", 35.0, 0);
    job_mgr.update_progress("Transcribing", 35.0);
    cache::update_state(video_path, model_id, lang, duration_secs, cache::PipelineState::Transcribing)?;

    // ── Stage 2: SenseVoice transcription ─────────────────────────────────────
    let threads = thermal::recommended_threads(opts.performance_mode);
    let output_dir = cache_dir.to_string_lossy().to_string();

    // Run sensevoice with retry
    let raw_segments = crate::utils::retry(|| async {
        let (tx, mut rx) = mpsc::channel::<sensevoice::SenseVoiceProgress>(32);
        let app_clone = app.clone();
        let jm_clone = job_mgr.clone();
        tokio::spawn(async move {
            while let Some(p) = rx.recv().await {
                let scaled = 35.0 + p.percent * 0.45; // 35% → 80%
                emit_progress(&app_clone, "Transcribing", scaled, 0);
                jm_clone.update_progress("Transcribing", scaled);
            }
        });

        let sidecar = app.shell().sidecar("sherpa-onnx").map_err(|e| 
            AutoSubError::SidecarNotFound(format!("sherpa-onnx sidecar not found: {}", e))
        )?;

        info!(
            "Transcription stage: starting transcription via sherpa-onnx for model {}", model_id
        );

        sensevoice::transcribe(
            sidecar,
            &model_config,
            &ModelManager::vad_model_path().to_string_lossy(),
            &final_audio,
            threads,
            Some(tx),
        ).await
    }, 2).await?;

    cache::update_state(video_path, model_id, lang, duration_secs, cache::PipelineState::Transcribed)?;

    emit_progress(&app, "Validating", 80.0, raw_segments.len());
    job_mgr.update_progress("Validating", 80.0);
    cache::update_state(video_path, model_id, lang, duration_secs, cache::PipelineState::Validating)?;

    // ── Stage 3: Validation ───────────────────────────────────────────────────
    let validated = validator::validate(raw_segments);

    emit_progress(&app, "Post-processing", 85.0, validated.len());
    job_mgr.update_progress("Post-processing", 85.0);

    // ── Stage 4: Post-processing ──────────────────────────────────────────────
    cache::update_state(video_path, model_name, lang, duration_secs, cache::PipelineState::Processing)?;
    let processed = post_process::process(validated);

    cache::update_state(video_path, model_name, lang, duration_secs, cache::PipelineState::Processed)?;

    emit_progress(&app, "Exporting", 95.0, processed.len());
    job_mgr.update_progress("Exporting", 95.0);
    cache::update_state(video_path, model_name, lang, duration_secs, cache::PipelineState::Exporting)?;

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
