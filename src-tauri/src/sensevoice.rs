use crate::error::{AutoSubError, Result};
use crate::model_manager::{ModelConfig, ModelKind};
use crate::subtitle::Segment;
use log::{debug, info, warn};
use serde::Deserialize;
use std::path::Path;
use tauri_plugin_shell::process::{Command, CommandEvent};
use tokio::sync::mpsc;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TranscribeProgress {
    pub percent: f32,
}

/// JSON line output từ sherpa-onnx-offline (khi có VAD)
#[derive(Debug, Deserialize)]
struct SherpaJsonSegment {
    text: String,
    #[serde(default)]
    start_time: f32,
    #[serde(default)]
    end_time: Option<f32>,
    #[serde(default)]
    duration: Option<f32>,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Transcribe audio file → Vec<Segment> với timestamps.
/// Tự động build CLI args dựa trên ModelKind.
pub async fn transcribe(
    sidecar: Command,
    config: &ModelConfig,
    vad_model_path: &str,
    audio_path: &str,
    threads: usize,
    progress_tx: Option<mpsc::Sender<TranscribeProgress>>,
) -> Result<Vec<Segment>> {
    // ── Validate inputs ───────────────────────────────────────────────────────
    if !Path::new(audio_path).exists() {
        return Err(AutoSubError::WhisperDecode(format!(
            "Audio file không tồn tại: {}", audio_path
        )));
    }

    let meta = std::fs::metadata(audio_path)
        .map_err(|e| AutoSubError::WhisperDecode(format!("Không đọc được audio: {}", e)))?;

    if meta.len() < 44 {
        return Err(AutoSubError::WhisperDecode(
            "Audio file quá nhỏ hoặc bị lỗi (< 44 bytes)".into(),
        ));
    }

    if !Path::new(vad_model_path).exists() {
        return Err(AutoSubError::WhisperDecode(format!(
            "VAD model không tồn tại: {}\nChạy: setup-models.sh --vad", vad_model_path
        )));
    }

    // ── Build CLI args ────────────────────────────────────────────────────────
    let args = build_args(config, vad_model_path, audio_path, threads)?;

    info!(
        "sensevoice: running sherpa-onnx [model={}] [threads={}] [audio={}]",
        config.id, threads, audio_path
    );
    debug!("sensevoice: args = {:?}", args);

    // ── Spawn process ─────────────────────────────────────────────────────────
    let (mut rx, _child) = sidecar
        .args(&args)
        .spawn()
        .map_err(|e| AutoSubError::WhisperDecode(
            format!("Không khởi động được sherpa-onnx: {}", e)
        ))?;

    // ── Collect output ────────────────────────────────────────────────────────
    let mut stdout_lines: Vec<String> = Vec::new();
    let mut stderr_buf: Vec<String> = Vec::new();
    let mut segment_count: usize = 0;

    loop {
        match rx.recv().await {
            Some(CommandEvent::Stdout(line)) => {
                let s = String::from_utf8_lossy(&line).trim().to_string();
                if !s.is_empty() {
                    // Đếm segments để ước tính progress
                    if s.starts_with('{') || s.contains("-->") || s.contains(" -- ") {
                        segment_count += 1;
                        if let Some(ref tx) = progress_tx {
                            // Ước tính: mỗi segment ~3s audio, 1 giờ = 1200 segments
                            let pct = ((segment_count as f32 / 200.0) * 90.0).min(90.0);
                            let _ = tx.send(TranscribeProgress { percent: pct }).await;
                        }
                    }
                    stdout_lines.push(s);
                }
            }
            Some(CommandEvent::Stderr(line)) => {
                let s = String::from_utf8_lossy(&line).trim().to_string();
                if !s.is_empty() {
                    // Filter verbose model loading logs
                    if !s.contains("OfflineRecognizerConfig")
                        && !s.contains("feat_config")
                        && !s.contains("model_config")
                        && !s.starts_with("I ")
                    {
                        debug!("sherpa stderr: {}", s);
                    }
                    stderr_buf.push(s);
                }
            }
            Some(CommandEvent::Terminated(payload)) => {
                if payload.code == Some(0) {
                    info!(
                        "sensevoice: hoàn thành — {} dòng output, ~{} segments",
                        stdout_lines.len(), segment_count
                    );
                    if let Some(ref tx) = progress_tx {
                        let _ = tx.send(TranscribeProgress { percent: 100.0 }).await;
                    }
                    break;
                } else {
                    // Lọc lấy lines hữu ích từ stderr
                    let relevant: Vec<String> = stderr_buf
                        .iter()
                        .filter(|l| {
                            let lo = l.to_lowercase();
                            lo.contains("error")
                                || lo.contains("failed")
                                || lo.contains("cannot")
                                || lo.contains("assert")
                                || lo.contains("invalid")
                        })
                        .cloned()
                        .take(8)
                        .collect();

                    let err_msg = if !relevant.is_empty() {
                        relevant.join("\n")
                    } else {
                        stderr_buf.iter().rev().take(5)
                            .cloned().collect::<Vec<_>>()
                            .into_iter().rev()
                            .collect::<Vec<_>>()
                            .join("\n")
                    };

                    return Err(AutoSubError::WhisperDecode(format!(
                        "sherpa-onnx thoát với mã lỗi {:?}\n{}",
                        payload.code, err_msg
                    )));
                }
            }
            None => {
                warn!("sensevoice: channel đóng bất ngờ");
                break;
            }
            _ => {}
        }
    }

    // ── Parse output → Segments ───────────────────────────────────────────────
    parse_output(&stdout_lines, &stderr_buf)
}

// ── Build CLI Args ────────────────────────────────────────────────────────────

fn build_args(
    config: &ModelConfig,
    vad_path: &str,
    audio_path: &str,
    threads: usize,
) -> Result<Vec<String>> {
    let mut args: Vec<String> = Vec::new();

    // Model-specific flags (phải đứng đầu)
    match &config.kind {
        ModelKind::SenseVoice { model, tokens } => {
            validate_path(model, "SenseVoice model")?;
            validate_path(tokens, "SenseVoice tokens")?;
            args.push(format!("--sense-voice={}", model.display()));
            args.push(format!("--tokens={}", tokens.display()));
            args.push("--use-itn=1".into()); // Inverse text normalization (số → chữ số)
        }
        ModelKind::FireRedAsr { encoder, decoder, tokens } => {
            validate_path(encoder, "FireRedASR encoder")?;
            validate_path(decoder, "FireRedASR decoder")?;
            validate_path(tokens, "FireRedASR tokens")?;
            args.push(format!("--fire-red-asr-encoder={}", encoder.display()));
            args.push(format!("--fire-red-asr-decoder={}", decoder.display()));
            args.push(format!("--tokens={}", tokens.display()));
        }
        ModelKind::Paraformer { model, tokens } => {
            validate_path(model, "Paraformer model")?;
            validate_path(tokens, "Paraformer tokens")?;
            args.push(format!("--paraformer={}", model.display()));
            args.push(format!("--tokens={}", tokens.display()));
        }
    }

    // VAD — BẮT BUỘC để lấy timestamps
    args.push(format!("--silero-vad-model={}", vad_path));
    args.push("--vad-min-silence-duration=0.3".into());
    args.push("--vad-max-speech-duration=29.0".into()); // Chia đoạn dài

    // Performance
    args.push(format!("--num-threads={}", threads));
    args.push("--debug=0".into());

    // Audio file — phải ở cuối
    args.push(audio_path.to_string());

    Ok(args)
}

fn validate_path(path: &std::path::PathBuf, label: &str) -> Result<()> {
    if !path.exists() {
        return Err(AutoSubError::WhisperDecode(format!(
            "{} không tồn tại tại: {}\nChạy setup-models.sh để tải model.",
            label,
            path.display()
        )));
    }
    Ok(())
}

// ── Output Parser ─────────────────────────────────────────────────────────────

/// Parse stdout của sherpa-onnx → Vec<Segment>
/// Hỗ trợ 2 format output:
///   1. JSON lines: {"text": "...", "start_time": X, "duration": Y}
///   2. Text:       "0.240 -- 3.520:  text here"  hoặc  "[ 0.240 -- 3.520 ]:  text"
fn parse_output(stdout: &[String], stderr: &[String]) -> Result<Vec<Segment>> {
    let mut segments: Vec<Segment> = Vec::new();

    for line in stdout {
        let line = line.trim();
        if line.is_empty() || line.starts_with('/') {
            // Bỏ qua: đường dẫn file input, dòng trống
            continue;
        }

        if let Some(seg) = try_parse_json(line) {
            if !seg.text.is_empty() {
                segments.push(seg);
            }
        } else if let Some(seg) = try_parse_text(line) {
            if !seg.text.is_empty() {
                segments.push(seg);
            }
        } else {
            debug!("parse: bỏ qua dòng không nhận dạng được: {:?}", line);
        }
    }

    info!("parse: {} segments từ {} dòng stdout", segments.len(), stdout.len());

    if segments.is_empty() && !stdout.is_empty() {
        // Giúp debug nếu output format khác dự kiến
        warn!(
            "parse: KHÔNG có segment nào được parse! Sample stdout ({} lines):\n{}",
            stdout.len(),
            stdout.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
        );

        // Nếu có stderr errors, ưu tiên báo error đó
        let err_lines: Vec<&str> = stderr
            .iter()
            .filter(|l| l.to_lowercase().contains("error"))
            .map(|s| s.as_str())
            .take(3)
            .collect();

        if !err_lines.is_empty() {
            return Err(AutoSubError::ParseFailed(format!(
                "sherpa-onnx không tạo ra segment nào.\nLỗi: {}",
                err_lines.join("\n")
            )));
        }

        // Không lỗi nhưng output trống → audio có thể không có giọng nói
        warn!("parse: Output trống — audio có thể không chứa giọng nói rõ ràng");
    }

    Ok(segments)
}

/// Format JSON: {"text": "...", "start_time": X, "end_time": Y} hoặc {"duration": Y}
fn try_parse_json(line: &str) -> Option<Segment> {
    if !line.starts_with('{') {
        return None;
    }

    let parsed: SherpaJsonSegment = serde_json::from_str(line).ok()?;
    let text = parsed.text.trim().to_string();
    if text.is_empty() {
        return None;
    }

    let end = parsed
        .end_time
        .or_else(|| parsed.duration.map(|d| parsed.start_time + d))
        .unwrap_or(parsed.start_time + 2.0); // fallback 2s nếu không có end/duration

    if end <= parsed.start_time {
        debug!("parse json: end <= start ({} <= {}), bỏ qua", end, parsed.start_time);
        return None;
    }

    Some(Segment {
        start: parsed.start_time,
        end,
        text,
    })
}

/// Format text: "0.240 -- 3.520:  text"  hoặc  "[ 0.240s -- 3.520s ]  text"
fn try_parse_text(line: &str) -> Option<Segment> {
    // Loại bỏ dấu ngoặc vuông nếu có
    let clean = line
        .trim_start_matches('[')
        .trim_end()
        .to_string();
    let clean = clean.replace(']', "");
    let clean = clean.trim().to_string();

    // Tách thời gian và text
    // Tìm dấu ":" cuối cùng của phần thời gian
    // Pattern: "X.XXX -- Y.YYY: text" hoặc "X.XXXs -- Y.YYYs text"
    let (time_part, text) = if let Some(colon_pos) = find_time_colon(&clean) {
        let t = clean[..colon_pos].trim();
        let tx = clean[colon_pos + 1..].trim().to_string();
        (t.to_string(), tx)
    } else {
        return None;
    };

    // Parse "X.XXX -- Y.YYY"
    let parts: Vec<&str> = time_part.splitn(2, "--").collect();
    if parts.len() != 2 {
        return None;
    }

    let start: f32 = parts[0]
        .trim()
        .trim_end_matches('s')
        .trim()
        .parse()
        .ok()?;
    let end: f32 = parts[1]
        .trim()
        .trim_end_matches('s')
        .trim()
        .parse()
        .ok()?;

    if text.is_empty() || end <= start {
        return None;
    }

    Some(Segment {
        start,
        end,
        text: text.trim().to_string(),
    })
}

/// Tìm vị trí ":" phân cách giữa timestamps và text
/// Ví dụ: "0.240 -- 3.520: text" → trả về index của ":"
fn find_time_colon(s: &str) -> Option<usize> {
    // Tìm "--" trước
    let dash_pos = s.find("--")?;

    // Tìm ":" sau "--"
    let after_dash = &s[dash_pos..];
    let colon_rel = after_dash.find(':')?;

    // Đảm bảo không có text phức tạp trước ":"
    let between = &after_dash[2..colon_rel]; // giữa "--" và ":"
    if between.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ' ' || c == 's') {
        Some(dash_pos + 2 + colon_rel)
    } else {
        None
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_with_end_time() {
        let line = r#"{"text": " 你好世界", "start_time": 0.24, "end_time": 3.52}"#;
        let seg = try_parse_json(line).unwrap();
        assert_eq!(seg.text, "你好世界");
        assert!((seg.start - 0.24).abs() < 0.001);
        assert!((seg.end - 3.52).abs() < 0.001);
    }

    #[test]
    fn test_parse_json_with_duration() {
        let line = r#"{"text": "Hello world", "start_time": 1.0, "duration": 2.5}"#;
        let seg = try_parse_json(line).unwrap();
        assert!((seg.end - 3.5).abs() < 0.001);
    }

    #[test]
    fn test_parse_text_format() {
        let line = "0.240 -- 3.520: 你好世界";
        let seg = try_parse_text(line).unwrap();
        assert_eq!(seg.text, "你好世界");
        assert!((seg.start - 0.240).abs() < 0.001);
        assert!((seg.end - 3.520).abs() < 0.001);
    }

    #[test]
    fn test_parse_text_with_seconds_suffix() {
        let line = "[ 0.240s -- 3.520s ]  Hello";
        let seg = try_parse_text(line).unwrap();
        assert_eq!(seg.text, "Hello");
        assert!((seg.start - 0.240).abs() < 0.001);
    }

    #[test]
    fn test_parse_empty_text_skipped() {
        let line = r#"{"text": "  ", "start_time": 0.0, "end_time": 1.0}"#;
        // Empty text after trim should be skipped by caller
        let seg = try_parse_json(line).unwrap();
        assert!(seg.text.is_empty()); // text là "" sau trim
    }

    #[test]
    fn test_parse_invalid_timestamps() {
        // end <= start → None
        let line = "3.520 -- 0.240: text";
        assert!(try_parse_text(line).is_none());
    }

    #[test]
    fn test_parse_output_mixed_formats() {
        let stdout = vec![
            "/path/to/audio.wav".to_string(), // phải bỏ qua
            r#"{"text": "first", "start_time": 0.0, "end_time": 2.0}"#.to_string(),
            "2.500 -- 5.000: second segment".to_string(),
            "".to_string(), // dòng trống
        ];
        let result = parse_output(&stdout, &[]).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "first");
        assert_eq!(result[1].text, "second segment");
    }
}
