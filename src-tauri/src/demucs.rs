use crate::error::{AutoSubError, Result};
use log::{info, debug};
use std::path::{Path, PathBuf};
use tauri_plugin_shell::process::{Command, CommandEvent};

/// Separate vocals from audio using Demucs.
/// Returns the path to the separated vocals file (vocals.wav).
pub async fn separate_vocals(
    sidecar: Command,
    audio_path: &str,
    output_dir: &str,
    model_path: &str,
) -> Result<String> {
    info!("demucs: Separating vocals from {}", audio_path);

    // Validate input audio exists
    if !Path::new(audio_path).exists() {
        return Err(AutoSubError::AudioExtract(format!(
            "Audio file not found at {}",
            audio_path
        )));
    }

    // Create output directory for demucs
    let out_dir_path = Path::new(output_dir).join("demucs_out");
    tokio::fs::create_dir_all(&out_dir_path)
        .await
        .map_err(|e| {
            AutoSubError::AudioExtract(format!("Failed to create demucs output dir: {}", e))
        })?;

    debug!(
        "demucs: Using model: {}, output: {}",
        model_path,
        out_dir_path.display()
    );

    // Build demucs command arguments
    // demucs-main [-m model.bin] [-i input.wav] [-o output_dir]
    let args = vec![
        "-m".to_string(),
        model_path.to_string(),
        "-i".to_string(),
        audio_path.to_string(),
        "-o".to_string(),
        out_dir_path.to_string_lossy().to_string(),
    ];

    let (mut rx, _child) = sidecar.args(&args).spawn().map_err(|e| {
        AutoSubError::AudioExtract(format!("Failed to spawn demucs sidecar: {}", e))
    })?;

    // Wait for demucs to complete
    loop {
        match rx.recv().await {
            Some(event) => match event {
                CommandEvent::Stdout(line) | CommandEvent::Stderr(line) => {
                    let line_str = String::from_utf8_lossy(&line);
                    debug!("demucs: {}", line_str.trim());
                }
                CommandEvent::Terminated(payload) => {
                    if payload.code == Some(0) {
                        info!("demucs: Successfully separated vocals");
                        break;
                    } else {
                        return Err(AutoSubError::AudioExtract(format!(
                            "Demucs failed with exit code: {:?}",
                            payload.code
                        )));
                    }
                }
                _ => {}
            },
            None => break,
        }
    }

    // Locate the separated vocals file
    // Demucs outputs to: output_dir/htdemucs/[input_name]/vocals.wav
    let vocals_path = find_vocals_file(&out_dir_path).await?;

    info!("demucs: Vocals extracted to {}", vocals_path.display());
    Ok(vocals_path.to_string_lossy().to_string())
}

/// Find the vocals.wav file in demucs output directory.
async fn find_vocals_file(base_dir: &Path) -> Result<PathBuf> {
    // Try common paths first: htdemucs/[something]/vocals.wav
    let htdemucs_dir = base_dir.join("htdemucs");
    if htdemucs_dir.exists() {
        // Look for vocals.wav in subdirectories
        if let Ok(mut entries) = tokio::fs::read_dir(&htdemucs_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(metadata) = entry.metadata().await {
                    if metadata.is_dir() {
                        let vocals_file = entry.path().join("vocals.wav");
                        if vocals_file.exists() {
                            return Ok(vocals_file);
                        }
                    }
                }
            }
        }
    }

    Err(AutoSubError::AudioExtract(
        "Could not find vocals.wav after demucs separation".to_string(),
    ))
}
