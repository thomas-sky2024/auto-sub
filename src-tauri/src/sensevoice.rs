// src-tauri/src/sensevoice.rs
// Dùng Python script generate-subtitles.py thay vì C++ binary
// Lý do: macOS không có binary sherpa-onnx-vad-with-offline-asr trong pre-built tarball

use crate::error::{AutoSubError, Result};
use crate::model_manager::{ModelConfig, ModelKind};
use crate::subtitle::Segment;
use log::{debug, info, warn};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, path::BaseDirectory};
use tauri_plugin_shell::process::{Command, CommandEvent};
use tokio::sync::mpsc;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TranscribeProgress {
    pub percent: f32,
}

/// JSON line output từ Python script
#[derive(Debug, Deserialize)]
struct ScriptSegment {
    start: f32,
    end: f32,
    text: String,
}

/// Lỗi từ script
#[derive(Debug, Deserialize)]
struct ScriptError {
    error: String,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Transcribe audio → Vec<Segment> dùng Python script qua Tauri shell sidecar.
/// 
/// Wrapper binary "sherpa-onnx-vad" trong binaries/ thực chất là shell script
/// gọi: python3 scripts/generate-subtitles.py [args...]
///
/// Output từ script: JSON lines trên stdout
///   {"start": 0.24, "end": 3.52, "text": "你好世界"}
pub async fn transcribe(
    app: &AppHandle,
    sidecar: Command,
    config: &ModelConfig,
    vad_model_path: &str,
    audio_path: &str,
    threads: usize,
    progress_tx: Option<mpsc::Sender<TranscribeProgress>>,
) -> Result<Vec<Segment>> {
    // ── Resolve script path ───────────────────────────────────────────────────
    let script_path = app.path()
        .resolve("scripts/generate-subtitles.py", BaseDirectory::Resource)
        .map_err(|e| AutoSubError::WhisperDecode(format!("Không tìm thấy script: {}", e)))?;

    // ── Validate inputs ───────────────────────────────────────────────────────
    validate_file(audio_path, "Audio", 44)?;
    validate_file(vad_model_path, "VAD model", 100_000)?;

    // ── Build args cho Python script ──────────────────────────────────────────
    let mut args = build_python_args(config, vad_model_path, audio_path, threads)?;
    
    // Chèn script path làm đối số đầu tiên cho wrapper
    args.insert(0, script_path.to_string_lossy().to_string());

    info!(
        "sensevoice: [model={}] [threads={}] audio={}",
        config.id, threads,
        Path::new(audio_path).file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    );
    debug!("sensevoice: args = {:?}", args);

    // ── Spawn ─────────────────────────────────────────────────────────────────
    let (mut rx, _child) = sidecar
        .args(&args)
        .spawn()
        .map_err(|e| AutoSubError::WhisperDecode(
            format!(
                "Không khởi động được sherpa-onnx-vad wrapper: {}\n\
                 Đảm bảo đã chạy: ./build-scripts/build-sensevoice-python.sh",
                e
            )
        ))?;

    // ── Collect output ────────────────────────────────────────────────────────
    // stdout: JSON lines mỗi segment
    // stderr: progress/debug info (bỏ qua hoặc log)
    let mut stdout_lines: Vec<String> = Vec::new();
    let mut stderr_buf: Vec<String> = Vec::new();
    let mut segment_count: usize = 0;

    loop {
        match rx.recv().await {
            Some(CommandEvent::Stdout(line)) => {
                let s = String::from_utf8_lossy(&line).trim().to_string();
                if !s.is_empty() {
                    // Đếm JSON lines để ước tính progress
                    if s.starts_with('{') {
                        segment_count += 1;
                        if let Some(ref tx) = progress_tx {
                            let pct = ((segment_count as f32 / 150.0) * 85.0).min(85.0);
                            let _ = tx.send(TranscribeProgress { percent: pct }).await;
                        }
                    }
                    stdout_lines.push(s);
                }
            }
            Some(CommandEvent::Stderr(line)) => {
                let s = String::from_utf8_lossy(&line).trim().to_string();
                if !s.is_empty() {
                    // Bỏ qua verbose output từ sherpa-onnx lib
                    if !s.starts_with("OfflineRecognizer")
                        && !s.contains("feat_config")
                        && !s.contains("num_threads")
                        && s.len() < 500
                    {
                        debug!("script stderr: {}", &s[..s.len().min(200)]);
                    }
                    stderr_buf.push(s);
                }
            }
            Some(CommandEvent::Terminated(payload)) => {
                match payload.code {
                    Some(0) => {
                        info!("sensevoice: OK — {} JSON lines, ~{} segments",
                            stdout_lines.len(), segment_count);
                        if let Some(ref tx) = progress_tx {
                            let _ = tx.send(TranscribeProgress { percent: 100.0 }).await;
                        }
                        break;
                    }
                    code => {
                        // Tìm thông điệp lỗi rõ ràng từ script
                        let script_err = stderr_buf.iter()
                            .filter(|l| l.starts_with('{'))
                            .find_map(|l| serde_json::from_str::<ScriptError>(l).ok())
                            .map(|e| e.error);

                        let err_msg = script_err.unwrap_or_else(|| {
                            stderr_buf.iter().rev().take(5)
                                .cloned().collect::<Vec<_>>()
                                .into_iter().rev()
                                .collect::<Vec<_>>()
                                .join("\n")
                        });

                        return Err(AutoSubError::WhisperDecode(format!(
                            "Python script thoát {:?}: {}",
                            code, err_msg
                        )));
                    }
                }
            }
            None => {
                warn!("sensevoice: channel đóng bất ngờ");
                break;
            }
            _ => {}
        }
    }

    // ── Parse JSON lines → Segments ───────────────────────────────────────────
    parse_json_output(&stdout_lines, &stderr_buf)
}

// ── Build Args ────────────────────────────────────────────────────────────────

/// Build args cho generate-subtitles.py
/// python3 generate-subtitles.py --model-dir X --model-type Y --vad Z --threads N audio.wav
fn build_python_args(
    config: &ModelConfig,
    vad_path: &str,
    audio_path: &str,
    threads: usize,
) -> Result<Vec<String>> {
    let mut args: Vec<String> = Vec::new();

    // --model-dir và --model-type theo ModelKind
    match &config.kind {
        ModelKind::SenseVoice { model, .. } => {
            let dir = model.parent()
                .ok_or_else(|| AutoSubError::WhisperDecode("Invalid model path".into()))?;
            validate_dir(dir, "SenseVoice model dir")?;
            args.push("--model-dir".into());
            args.push(dir.to_string_lossy().to_string());
            args.push("--model-type".into());
            args.push("sense-voice".into());
        }
        ModelKind::Paraformer { model, .. } => {
            let dir = model.parent()
                .ok_or_else(|| AutoSubError::WhisperDecode("Invalid model path".into()))?;
            validate_dir(dir, "Paraformer model dir")?;
            args.push("--model-dir".into());
            args.push(dir.to_string_lossy().to_string());
            args.push("--model-type".into());
            args.push("paraformer".into());
        }
        ModelKind::FireRedAsr { encoder, .. } => {
            let dir = encoder.parent()
                .ok_or_else(|| AutoSubError::WhisperDecode("Invalid model path".into()))?;
            validate_dir(dir, "FireRedASR model dir")?;
            args.push("--model-dir".into());
            args.push(dir.to_string_lossy().to_string());
            args.push("--model-type".into());
            args.push("fire-red-v2".into());
        }
    }

    args.push("--vad".into());
    args.push(vad_path.to_string());

    args.push("--threads".into());
    args.push(threads.to_string());

    // Audio file — phải ở cuối
    args.push(audio_path.to_string());

    Ok(args)
}

// ── Output Parser ─────────────────────────────────────────────────────────────

/// Parse JSON lines từ stdout Python script
/// Mỗi dòng: {"start": X, "end": Y, "text": "..."}
fn parse_json_output(stdout: &[String], stderr: &[String]) -> Result<Vec<Segment>> {
    let mut segments: Vec<Segment> = Vec::new();

    for line in stdout {
        let line = line.trim();
        if line.is_empty() || !line.starts_with('{') {
            continue;
        }

        // Thử parse như segment trước
        if let Ok(seg) = serde_json::from_str::<ScriptSegment>(line) {
            let text = seg.text.trim().to_string();
            if !text.is_empty() && seg.end > seg.start && seg.start >= 0.0 {
                segments.push(Segment {
                    start: seg.start,
                    end: seg.end,
                    text,
                });
            }
        } else if let Ok(err) = serde_json::from_str::<ScriptError>(line) {
            // Script báo lỗi
            return Err(AutoSubError::ParseFailed(format!(
                "Script error: {}", err.error
            )));
        } else {
            debug!("parse: bỏ qua JSON không nhận dạng: {:?}", &line[..line.len().min(100)]);
        }
    }

    info!("parse: {} segments từ {} stdout lines", segments.len(), stdout.len());

    if segments.is_empty() && !stdout.is_empty() {
        warn!(
            "parse: KHÔNG parse được segment nào!\nFirst 5 stdout lines:\n{}",
            stdout.iter().take(5)
                .map(|s| format!("  {:?}", &s[..s.len().min(100)]))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    // Kiểm tra lỗi Python install
    let pip_err = stderr.iter()
        .any(|l| l.contains("No module named") || l.contains("ImportError"));

    if pip_err && segments.is_empty() {
        return Err(AutoSubError::WhisperDecode(
            "sherpa_onnx Python package chưa được cài.\n\
             Chạy: pip3 install sherpa-onnx numpy".into()
        ));
    }

    Ok(segments)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn validate_file(path: &str, label: &str, min_bytes: u64) -> Result<()> {
    let meta = std::fs::metadata(path).map_err(|_| {
        AutoSubError::WhisperDecode(format!("{} không tồn tại: {}", label, path))
    })?;
    if meta.len() < min_bytes {
        return Err(AutoSubError::WhisperDecode(format!(
            "{} quá nhỏ ({} bytes): {}", label, meta.len(), path
        )));
    }
    Ok(())
}

fn validate_dir(path: &Path, label: &str) -> Result<()> {
    if !path.exists() || !path.is_dir() {
        return Err(AutoSubError::WhisperDecode(format!(
            "{} không tồn tại: {}\nChạy: ./build-scripts/setup-models.sh",
            label, path.display()
        )));
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_json_segment() {
        let lines = vec![
            r#"{"start": 0.24, "end": 3.52, "text": "你好世界"}"#.to_string(),
        ];
        let result = parse_json_output(&lines, &[]).unwrap();
        assert_eq!(result.len(), 1);
        assert!((result[0].start - 0.24).abs() < 0.001);
        assert!((result[0].end - 3.52).abs() < 0.001);
        assert_eq!(result[0].text, "你好世界");
    }

    #[test]
    fn test_skip_empty_text() {
        let lines = vec![
            r#"{"start": 0.0, "end": 1.0, "text": "  "}"#.to_string(),
            r#"{"start": 1.0, "end": 2.0, "text": "hello"}"#.to_string(),
        ];
        let result = parse_json_output(&lines, &[]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "hello");
    }

    #[test]
    fn test_skip_invalid_timestamps() {
        let lines = vec![
            r#"{"start": 5.0, "end": 2.0, "text": "reversed"}"#.to_string(), // end < start
            r#"{"start": 1.0, "end": 3.0, "text": "valid"}"#.to_string(),
        ];
        let result = parse_json_output(&lines, &[]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "valid");
    }

    #[test]
    fn test_script_error_json() {
        let lines = vec![
            r#"{"error": "sherpa_onnx chưa được cài"}"#.to_string(),
        ];
        let result = parse_json_output(&lines, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Script error"));
    }

    #[test]
    fn test_skip_non_json_lines() {
        let lines = vec![
            "Loading model...".to_string(),
            r#"{"start": 0.5, "end": 2.5, "text": "test"}"#.to_string(),
            "Done".to_string(),
        ];
        let result = parse_json_output(&lines, &[]).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_pip_error_detection() {
        let lines = vec![];
        let stderr = vec!["ModuleNotFoundError: No module named 'sherpa_onnx'".to_string()];
        let result = parse_json_output(&lines, &stderr);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_segments() {
        let lines = vec![
            r#"{"start": 0.0, "end": 2.0, "text": "first"}"#.to_string(),
            r#"{"start": 3.0, "end": 5.0, "text": "second"}"#.to_string(),
            r#"{"start": 6.0, "end": 8.0, "text": "third"}"#.to_string(),
        ];
        let result = parse_json_output(&lines, &[]).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[2].text, "third");
    }
}
