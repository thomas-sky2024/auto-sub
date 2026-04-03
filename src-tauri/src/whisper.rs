use crate::error::{AutoSubError, Result};
use crate::model_manager::ModelManager;
use crate::subtitle::Segment;
use log::{info, warn};
use serde::Deserialize;
use std::path::Path;
use tauri_plugin_shell::process::{Command, CommandEvent};
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
/// Parse whisper timestamp "HH:MM:SS.mmm" or "HH:MM:SS,mmm" into seconds.
fn parse_timestamp(ts: &str) -> f32 {
    // Chuyển dấu phẩy thành dấu chấm để hàm f32::parse() hoạt động đúng
    let normalized = ts.replace(',', ".");
    let parts: Vec<&str> = normalized.split(':').collect();
    
    if parts.len() != 3 {
        return 0.0;
    }
    
    let h: f32 = parts[0].parse().unwrap_or(0.0);
    let m: f32 = parts[1].parse().unwrap_or(0.0);
    let s: f32 = parts[2].parse().unwrap_or(0.0);
    
    h * 3600.0 + m * 60.0 + s
}

/// Run whisper-main CLI on an audio file.
pub async fn transcribe(
    sidecar: Command,
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
            "Audio file is empty. Source video may have no audio or FFmpeg extraction failed."
                .to_string(),
        ));
    }

    if audio_metadata.len() < 44 {
        return Err(AutoSubError::WhisperDecode(format!(
            "Audio file is too small ({} bytes) - likely invalid or corrupted.",
            audio_metadata.len()
        )));
    }

    run_whisper(
        sidecar,
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
    sidecar: Command,
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
        "-oj".to_string(), // Output JSON
        "-of".to_string(),
        output_base.clone(),
        "-bs".to_string(),
        "5".to_string(), // Beam size 5
        "-t".to_string(),
        threads.to_string(),
        "--max-len".to_string(),
        "60".to_string(),
        "--temperature".to_string(),
        "0".to_string(),
        "--print-progress".to_string(),
    ];

    // Enable Silero VAD for voice activity detection (filters silence = faster processing)
    if ModelManager::vad_model_ready() {
        let vad_path = ModelManager::get_vad_model_path();
        args.extend([
            "--vad".to_string(),
            vad_path.to_string_lossy().to_string(),
        ]);
        info!("whisper: Silero VAD enabled for silence filtering");
    } else {
        warn!("whisper: Silero VAD model not found, processing without VAD. Download it with setup-models.sh for faster transcription.");
    }

    // Add language flag
    if language == "auto" || language.is_empty() {
        args.extend(["-l".to_string(), "auto".to_string()]);
    } else {
        args.extend(["-l".to_string(), language.to_string()]);
    }

    let (mut rx, _child) = sidecar.args(&args).spawn().map_err(|e| {
        AutoSubError::WhisperDecode(format!("Failed to spawn whisper sidecar: {}", e))
    })?;

    let mut stderr_lines: Vec<String> = Vec::new();

    loop {
        match rx.recv().await {
            Some(event) => match event {
                // whisper.cpp writes progress to STDERR, not stdout
                CommandEvent::Stderr(line) => {
                    let line_str = String::from_utf8_lossy(&line).to_string();

                    // Parse progress: whisper outputs "whisper_full: progress = XX%"
                    if line_str.contains("progress =") {
                        if let Some(pct_str) = line_str.split('=').nth(1) {
                            let pct_clean = pct_str.trim().trim_end_matches('%');
                            if let Ok(pct) = pct_clean.parse::<f32>() {
                                if let Some(ref tx) = progress_tx {
                                    let _ = tx.send(WhisperProgress { percent: pct }).await;
                                }
                            }
                        }
                    }

                    // Collect stderr for error reporting
                    stderr_lines.push(line_str);
                }

                CommandEvent::Stdout(line) => {
                    // whisper may also write some info to stdout; collect for debugging
                    let line_str = String::from_utf8_lossy(&line).to_string();
                    if line_str.contains("progress =") {
                        if let Some(pct_str) = line_str.split('=').nth(1) {
                            let pct_clean = pct_str.trim().trim_end_matches('%');
                            if let Ok(pct) = pct_clean.parse::<f32>() {
                                if let Some(ref tx) = progress_tx {
                                    let _ = tx.send(WhisperProgress { percent: pct }).await;
                                }
                            }
                        }
                    }
                }

                CommandEvent::Terminated(payload) => {
                    if payload.code == Some(0) {
                        info!("whisper: process terminated successfully");
                        break;
                    } else {
                        // Filter stderr to the most useful lines (skip verbose model-loading lines)
                        let relevant_stderr: Vec<&str> = stderr_lines
                            .iter()
                            .filter(|l| {
                                let l = l.to_lowercase();
                                l.contains("error")
                                    || l.contains("failed")
                                    || l.contains("assert")
                                    || l.contains("cannot")
                            })
                            .map(|s| s.as_str())
                            .take(10)
                            .collect();

                        let stderr_msg = if !relevant_stderr.is_empty() {
                            format!("\nwhisper stderr:\n{}", relevant_stderr.join("\n"))
                        } else if !stderr_lines.is_empty() {
                            // Fall back to last few lines
                            let tail: Vec<&str> = stderr_lines
                                .iter()
                                .rev()
                                .take(5)
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                                .collect();
                            format!("\nwhisper stderr (last 5 lines):\n{}", tail.join("\n"))
                        } else {
                            String::new()
                        };

                        return Err(AutoSubError::WhisperDecode(format!(
                            "whisper exited with code: {:?}{}",
                            payload.code, stderr_msg
                        )));
                    }
                }
                _ => {}
            },
            None => {
                // If the channel closes but we haven't seen a Success termination,
                // it likely means the process crashed or was killed prematurely.
                warn!("whisper: event channel closed unexpectedly - process may have crashed");
                
                let stderr_msg = if !stderr_lines.is_empty() {
                    let tail: Vec<&str> = stderr_lines
                        .iter()
                        .rev()
                        .take(10)
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    format!("\nwhisper stderr (last 10 lines):\n{}", tail.join("\n"))
                } else {
                    "No stderr available.".to_string()
                };

                return Err(AutoSubError::WhisperDecode(format!(
                    "whisper process disconnected prematurely. Possible OOM or crash.{}",
                    stderr_msg
                )));
            }
        }
    }

    // ── Parse output JSON ──────────────────────────────────────────────────────
    let json_path = format!("{}.json", output_base);
    if !Path::new(&json_path).exists() {
        let output_dir_path = Path::new(output_dir);
        let files_in_dir = std::fs::read_dir(output_dir_path)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_else(|| "Could not read directory".to_string());

        let stderr_msg = if !stderr_lines.is_empty() {
            let tail: Vec<&str> = stderr_lines
                .iter()
                .rev()
                .take(8)
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("\nwhisper stderr:\n{}", tail.join("\n"))
        } else {
            String::new()
        };

        return Err(AutoSubError::ParseFailed(format!(
            "whisper did not produce output.json at {}.\nFiles in output dir: [{}]\n\
             Possible causes: invalid audio format, corrupted audio, or whisper crash.{}",
            json_path, files_in_dir, stderr_msg
        )));
    }

    let json_str = tokio::fs::read_to_string(&json_path)
        .await
        .map_err(|e| AutoSubError::ParseFailed(format!("Failed to read output JSON: {}", e)))?;

    // Validate JSON completeness before parsing
    let trimmed_json = json_str.trim();
    if !trimmed_json.starts_with('{') || !trimmed_json.ends_with('}') {
        return Err(AutoSubError::ParseFailed(
            "Whisper output JSON is incomplete or corrupt (missing braces). \
             The process may have been interrupted."
                .to_string(),
        ));
    }

    let whisper_output: WhisperOutput = serde_json::from_str(trimmed_json).map_err(|e| {
        AutoSubError::ParseFailed(format!(
            "Failed to parse whisper JSON: {}. First 200 chars: {}",
            e,
            &trimmed_json[..trimmed_json.len().min(200)]
        ))
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

    info!(
        "whisper: transcription complete, {} segments",
        segments.len()
    );
    Ok(segments)
}
