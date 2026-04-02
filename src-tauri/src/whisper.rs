use crate::error::{AutoSubError, Result};
use crate::subtitle::Segment;
use log::info;
use serde::Deserialize;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

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
    // Validate audio file exists and has content
    if !Path::new(audio_path).exists() {
        return Err(AutoSubError::WhisperDecode(format!(
            "Audio file not found at {}. FFmpeg extraction may have failed.",
            audio_path
        )));
    }

    let audio_metadata = std::fs::metadata(audio_path).map_err(|e| {
        AutoSubError::WhisperDecode(format!("Failed to read audio file metadata: {}", e))
    })?;

    if audio_metadata.len() == 0 {
        return Err(AutoSubError::WhisperDecode(
            "Audio file is empty. Source video may have no audio or FFmpeg extraction failed.".to_string(),
        ));
    }

    if audio_metadata.len() < 44 {
        return Err(AutoSubError::WhisperDecode(format!(
            "Audio file is too small ({} bytes) - likely invalid or corrupted.",
            audio_metadata.len()
        )));
    }

    run_whisper(
        whisper_bin,
        model_path,
        audio_path,
        output_dir,
        language,
        threads,
        &progress_tx,
    )
    .await
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
        "--max-len".to_string(),
        "60".to_string(),
        "--temperature".to_string(),
        "0".to_string(),
        "--vad".to_string(), // Voice Activity Detection for suppression of silence
        "--print-progress".to_string(),
    ];

    // Add language flag
    if language == "auto" || language.is_empty() {
        args.extend(["-l".to_string(), "auto".to_string()]);
    } else {
        args.extend(["-l".to_string(), language.to_string()]);
    }

    if !Path::new(whisper_bin).exists() {
        return Err(AutoSubError::SidecarNotFound(format!(
            "whisper-main not found at {}. Please check your installation.",
            whisper_bin
        )));
    }

    let mut child = Command::new(whisper_bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AutoSubError::WhisperDecode(format!("Failed to spawn whisper: {}", e)))?;

    let (stderr_tx, mut stderr_rx) = mpsc::channel::<String>(256);

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

    // Capture stderr for error reporting
    if let Some(stderr) = child.stderr.take() {
        let stderr_tx = stderr_tx.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = stderr_tx.send(line).await;
            }
        });
    }

    let status = child.wait().await.map_err(|e| {
        AutoSubError::WhisperDecode(format!("whisper process error: {}", e))
    })?;

    // Collect any stderr output that was captured
    let mut stderr_output = Vec::new();
    while let Ok(Some(line)) = timeout(Duration::from_millis(100), stderr_rx.recv()).await {
        stderr_output.push(line);
    }

    if !status.success() {
        let stderr_msg = if !stderr_output.is_empty() {
            format!("\nwhisper stderr: {}", stderr_output.join("\n"))
        } else {
            String::new()
        };

        return Err(AutoSubError::WhisperDecode(format!(
            "whisper exited with code: {:?}{}",
            status.code(),
            stderr_msg
        )));
    }

    // Parse output JSON
    let json_path = format!("{}.json", output_base);
    if !Path::new(&json_path).exists() {
        // Check what files were actually created
        let output_dir_path = Path::new(output_dir);
        let files_in_dir = std::fs::read_dir(output_dir_path)
            .ok()
            .and_then(|entries| {
                let filenames: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect();
                if filenames.is_empty() {
                    None
                } else {
                    Some(filenames.join(", "))
                }
            })
            .unwrap_or_else(|| "Could not read directory".to_string());

        let stderr_msg = if !stderr_output.is_empty() {
            format!("\nwhisper stderr: {}", stderr_output.join("\n"))
        } else {
            String::new()
        };

        return Err(AutoSubError::ParseFailed(format!(
            "whisper did not produce output.json at {}. Files in output dir: {}. \
             Possible causes: invalid audio format, corrupted audio file, or whisper process error.{}",
            json_path, files_in_dir, stderr_msg
        )));
    }

    let json_str = tokio::fs::read_to_string(&json_path).await.map_err(|e| {
        AutoSubError::ParseFailed(format!("Failed to read output JSON: {}", e))
    })?;

    // Robustness: Validate JSON is complete
    let trimmed_json = json_str.trim();
    if !trimmed_json.ends_with('}') || !trimmed_json.starts_with('{') {
        return Err(AutoSubError::ParseFailed(
            "Whisper output JSON is incomplete or corrupt (missing braces)".to_string(),
        ));
    }

    let whisper_output: WhisperOutput = serde_json::from_str(trimmed_json).map_err(|e| {
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
