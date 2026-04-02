use crate::error::{AutoSubError, Result};
use log::{debug, error, info, warn};
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
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
    ffmpeg_bin: &str,
    video_path: &str,
    output_path: &str,
    video_duration_secs: f32,
    progress_tx: Option<mpsc::Sender<FfmpegProgress>>,
) -> Result<()> {
    run_ffmpeg(ffmpeg_bin, video_path, output_path, video_duration_secs, &progress_tx).await
}

async fn run_ffmpeg(
    ffmpeg_bin: &str,
    video_path: &str,
    output_path: &str,
    video_duration_secs: f32,
    progress_tx: &Option<mpsc::Sender<FfmpegProgress>>,
) -> Result<()> {
    info!("ffmpeg: extracting audio from {}", video_path);

    if !Path::new(ffmpeg_bin).exists() {
        return Err(AutoSubError::SidecarNotFound(format!(
            "ffmpeg not found at {}. Please check your installation.",
            ffmpeg_bin
        )));
    }

    if !Path::new(video_path).exists() {
        return Err(AutoSubError::AudioExtract(format!(
            "Input file not found: {}",
            video_path
        )));
    }

    let mut child = Command::new(ffmpeg_bin)
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
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            AutoSubError::AudioExtract(format!("Failed to spawn ffmpeg: {}", e))
        })?;

    // Parse stderr for progress (out_time_ms)
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        let inactivity_timeout = Duration::from_secs(30);

        loop {
            match timeout(inactivity_timeout, lines.next_line()).await {
                Ok(Ok(Some(line))) => {
                    if line.starts_with("out_time_ms=") {
                        if let Ok(us) = line
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
                Ok(Ok(None)) => break, // EOF
                Ok(Err(e)) => {
                    warn!("ffmpeg stderr read error: {}", e);
                    break;
                }
                Err(_) => {
                    // Timeout — kill ffmpeg
                    error!("ffmpeg inactivity timeout (30s), killing process");
                    let _ = child.kill().await;
                    return Err(AutoSubError::AudioExtract(
                        "ffmpeg timed out (30s no output)".to_string(),
                    ));
                }
            }
        }
    }

    let status = child.wait().await.map_err(|e| {
        AutoSubError::AudioExtract(format!("ffmpeg process error: {}", e))
    })?;

    if !status.success() {
        return Err(AutoSubError::AudioExtract(format!(
            "ffmpeg exited with code: {:?}",
            status.code()
        )));
    }

    info!("ffmpeg: audio extraction complete → {}", output_path);
    Ok(())
}

/// Get video duration in seconds using ffprobe.
pub async fn get_video_duration(ffmpeg_bin: &str, video_path: &str) -> Result<f32> {
    // Derive ffprobe path from ffmpeg path
    let ffprobe_bin = ffmpeg_bin.replace("ffmpeg", "ffprobe");
    let ffprobe_path = if Path::new(&ffprobe_bin).exists() {
        ffprobe_bin
    } else {
        "ffprobe".to_string() // fallback to system ffprobe
    };

    if !Path::new(&ffprobe_path).exists() && ffprobe_path != "ffprobe" {
         return Err(AutoSubError::SidecarNotFound(format!(
            "ffprobe not found at {}. Please check your installation.",
            ffprobe_path
        )));
    }

    let output = Command::new(&ffprobe_path)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
            video_path,
        ])
        .output()
        .await
        .map_err(|e| {
            AutoSubError::AudioExtract(format!("Failed to run ffprobe: {}", e))
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .trim()
        .parse::<f32>()
        .map_err(|e| AutoSubError::AudioExtract(format!("Failed to parse duration: {}", e)))
}
