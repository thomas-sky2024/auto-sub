use crate::error::{AutoSubError, Result};
use crate::subtitle::Segment;
use log::{debug, error, info, warn};
use serde::Deserialize;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Progress update from whisper.
#[derive(Debug, Clone)]
pub struct WhisperProgress {
    pub percent: f32,
}

/// whisper.cpp JSON output structure
#[derive(Debug, Deserialize)]
struct WhisperOutput {
    transcription: Vec<WhisperSegment>,
}

#[derive(Debug, Deserialize)]
struct WhisperSegment {
    timestamps: WhisperTimestamps,
    text: String,
}

#[derive(Debug, Deserialize)]
struct WhisperTimestamps {
    from: String,
    to: String,
}

/// Parse whisper timestamp "HH:MM:SS.mmm" into seconds.
fn parse_timestamp(ts: &str) -> f32 {
    let parts: Vec<&str> = ts.split(':').collect();
    if parts.len() != 3 {
        return 0.0;
    }
    let h: f32 = parts[0].parse().unwrap_or(0.0);
    let m: f32 = parts[1].parse().unwrap_or(0.0);
    let s: f32 = parts[2].parse().unwrap_or(0.0);
    h * 3600.0 + m * 60.0 + s
}

/// Run whisper-main CLI on an audio file.
/// Retries once on failure, then falls back to `small` model.
pub async fn transcribe(
    whisper_bin: &str,
    model_path: &str,
    audio_path: &str,
    output_dir: &str,
    language: &str,
    threads: usize,
    progress_tx: Option<mpsc::Sender<WhisperProgress>>,
) -> Result<Vec<Segment>> {
    // Try with primary model
    match run_whisper(
        whisper_bin,
        model_path,
        audio_path,
        output_dir,
        language,
        threads,
        &progress_tx,
    )
    .await
    {
        Ok(segments) => return Ok(segments),
        Err(e) => {
            warn!("Whisper first attempt failed: {}, retrying…", e);
        }
    }

    // Retry once with same model
    match run_whisper(
        whisper_bin,
        model_path,
        audio_path,
        output_dir,
        language,
        threads,
        &progress_tx,
    )
    .await
    {
        Ok(segments) => return Ok(segments),
        Err(e) => {
            warn!("Whisper retry failed: {}", e);
        }
    }

    // Fallback to small model if available
    let small_model = model_path.replace("medium", "small");
    if Path::new(&small_model).exists() {
        warn!("Falling back to small model: {}", small_model);
        run_whisper(
            whisper_bin,
            &small_model,
            audio_path,
            output_dir,
            language,
            threads,
            &progress_tx,
        )
        .await
    } else {
        Err(AutoSubError::WhisperDecode(
            "All whisper attempts failed or source file corrupt".to_string(),
        ))
    }
}

async fn run_whisper(
    whisper_bin: &str,
    model_path: &str,
    audio_path: &str,
    output_dir: &str,
    language: &str,
    threads: usize,
    progress_tx: &Option<mpsc::Sender<WhisperProgress>>,
) -> Result<Vec<Segment>> {
    let output_base = format!("{}/output", output_dir);

    info!(
        "whisper: transcribing {} with model {} (lang={}, threads={})",
        audio_path, model_path, language, threads
    );

    let mut args = vec![
        "-m".to_string(),
        model_path.to_string(),
        "-f".to_string(),
        audio_path.to_string(),
        "-oj".to_string(),
        "-of".to_string(),
        output_base.clone(),
        "-bs".to_string(),
        "5".to_string(),
        "-t".to_string(),
        threads.to_string(),
        "--no-context".to_string(),
        "--max-len".to_string(),
        "60".to_string(),
        "--temperature".to_string(),
        "0".to_string(),
        "-vad".to_string(), // Voice Activity Detection for suppression of silence
        "--print-progress".to_string(),
    ];

    // Add language flag
    if language == "auto" || language.is_empty() {
        args.extend(["-l".to_string(), "auto".to_string()]);
    } else {
        args.extend(["-l".to_string(), language.to_string()]);
    }

    let mut child = Command::new(whisper_bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AutoSubError::WhisperDecode(format!("Failed to spawn whisper: {}", e)))?;

    // Parse stdout/stderr for progress
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let progress_tx = progress_tx.clone();

        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                // whisper --print-progress outputs "progress = XX%"
                if line.contains("progress =") {
                    if let Some(pct_str) = line.split('=').nth(1) {
                        if let Ok(pct) = pct_str.trim().trim_end_matches('%').parse::<f32>() {
                            if let Some(ref tx) = progress_tx {
                                let _ = tx.send(WhisperProgress { percent: pct }).await;
                            }
                        }
                    }
                }
            }
        });
    }

    let status = child.wait().await.map_err(|e| {
        AutoSubError::WhisperDecode(format!("whisper process error: {}", e))
    })?;

    if !status.success() {
        return Err(AutoSubError::WhisperDecode(format!(
            "whisper exited with code: {:?}",
            status.code()
        )));
    }

    // Parse output JSON
    let json_path = format!("{}.json", output_base);
    if !Path::new(&json_path).exists() {
        return Err(AutoSubError::ParseFailed(format!(
            "Output JSON not found at {}",
            json_path
        )));
    }

    let json_str = tokio::fs::read_to_string(&json_path).await.map_err(|e| {
        AutoSubError::ParseFailed(format!("Failed to read output JSON: {}", e))
    })?;

    let whisper_output: WhisperOutput = serde_json::from_str(&json_str).map_err(|e| {
        AutoSubError::ParseFailed(format!("Failed to parse whisper JSON: {}", e))
    })?;

    let segments: Vec<Segment> = whisper_output
        .transcription
        .into_iter()
        .map(|ws| Segment {
            start: parse_timestamp(&ws.timestamps.from),
            end: parse_timestamp(&ws.timestamps.to),
            text: ws.text.trim().to_string(),
        })
        .collect();

    info!("whisper: transcription complete, {} segments", segments.len());
    Ok(segments)
}
