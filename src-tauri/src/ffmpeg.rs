use crate::error::{AutoSubError, Result};
use log::{debug, error, info};
use std::path::Path;
use tauri_plugin_shell::process::{Command, CommandEvent};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

/// Progress update from ffmpeg extraction.
#[derive(Debug, Clone)]
pub struct FfmpegProgress {
    pub percent: f32,
}

/// Extract audio from video as raw PCM s16le to a WAV-like file.
/// Uses ffmpeg with `-progress pipe:2` for real progress tracking.
/// Retries up to 2 times on failure.
pub async fn extract_audio(
    sidecar: Command,
    video_path: &str,
    output_path: &str,
    video_duration_secs: f32,
    progress_tx: Option<mpsc::Sender<FfmpegProgress>>,
) -> Result<()> {
    run_ffmpeg(sidecar, video_path, output_path, video_duration_secs, &progress_tx).await
}

async fn run_ffmpeg(
    sidecar: Command,
    video_path: &str,
    output_path: &str,
    video_duration_secs: f32,
    progress_tx: &Option<mpsc::Sender<FfmpegProgress>>,
) -> Result<()> {
    info!("ffmpeg: extracting audio from {} to {}", video_path, output_path);

    if !Path::new(video_path).exists() {
        return Err(AutoSubError::AudioExtract(format!(
            "Input file not found: {}",
            video_path
        )));
    }

    let (mut rx, child) = sidecar
        .args([
            "-y",
            "-i",
            video_path,
            "-vn",
            "-acodec",
            "pcm_s16le",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-f",
            "wav",
            output_path,
            "-progress",
            "pipe:2",
        ])
        .spawn()
        .map_err(|e| {
            AutoSubError::AudioExtract(format!("Failed to spawn ffmpeg sidecar: {}", e))
        })?;

    // Use a timeout for the entire process if it stays idle
    let inactivity_timeout = Duration::from_secs(30);

    loop {
        match timeout(inactivity_timeout, rx.recv()).await {
            Ok(Some(event)) => {
                match event {
                    CommandEvent::Stderr(line) => {
                        let line_str = String::from_utf8_lossy(&line);
                        if line_str.starts_with("out_time_ms=") {
                            if let Ok(us) = line_str
                                .trim_start_matches("out_time_ms=")
                                .trim()
                                .parse::<i64>()
                            {
                                let secs = us as f32 / 1_000_000.0;
                                let pct = if video_duration_secs > 0.0 {
                                    (secs / video_duration_secs * 100.0).min(100.0)
                                } else {
                                    0.0
                                };
                                debug!("ffmpeg progress: {:.1}%", pct);
                                if let Some(tx) = progress_tx {
                                    let _ = tx.send(FfmpegProgress { percent: pct }).await;
                                }
                            }
                        }
                    }
                    CommandEvent::Terminated(payload) => {
                        if payload.code == Some(0) {
                            info!("ffmpeg: audio extraction complete → {}", output_path);
                            return Ok(());
                        } else {
                            error!("ffmpeg exited with code: {:?}", payload.code);
                            return Err(AutoSubError::AudioExtract(format!(
                                "ffmpeg exited with code: {:?}",
                                payload.code
                            )));
                        }
                    }
                    _ => {}
                }
            }
            Ok(None) => break, // Receiver closed
            Err(_) => {
                // Timeout — kill ffmpeg
                error!("ffmpeg inactivity timeout (30s), killing process");
                let _ = child.kill();
                return Err(AutoSubError::AudioExtract(
                    "ffmpeg timed out (30s no output)".to_string(),
                ));
            }
        }
    }

    Ok(())
}

/// Get video duration in seconds using ffprobe.
pub async fn get_video_duration(sidecar: Command, video_path: &str) -> Result<f32> {
    let (mut rx, _child) = sidecar
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
            video_path,
        ])
        .spawn()
        .map_err(|e| {
            AutoSubError::AudioExtract(format!("Failed to spawn ffprobe sidecar: {}", e))
        })?;

    let mut output = String::new();
    while let Some(event) = rx.recv().await {
        if let CommandEvent::Stdout(line) = event {
            output.push_str(&String::from_utf8_lossy(&line));
        }
    }

    output
        .trim()
        .parse::<f32>()
        .map_err(|e| AutoSubError::AudioExtract(format!("Failed to parse duration: {}", e)))
}
