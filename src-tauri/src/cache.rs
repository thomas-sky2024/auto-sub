use crate::error::{AutoSubError, Result};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::fs;

const PIPELINE_VERSION: &str = "v5";
const WHISPER_VERSION: &str = "1.8.4";

/// Current stage of the pipeline for idempotency.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineState {
    Extracting,
    Extracted,
    Transcribing,
    Transcribed,
    Validating,
    Validated,
    Processing,
    Processed,
    Exporting,
    Completed,
    Failed,
}

/// Cache metadata for validation.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CacheMeta {
    pub model: String,
    pub lang: String,
    pub duration: f32,
    pub whisper_version: String,
    pub pipeline_version: String,
    pub state: PipelineState,
}

/// Get the cache directory for a given video file.
pub fn cache_dir(video_path: &str) -> Result<PathBuf> {
    let hash = compute_file_hash(video_path)?;
    let base = dirs::home_dir()
        .ok_or_else(|| AutoSubError::Cache("Cannot determine home directory".into()))?;
    Ok(base.join(".autosub").join("cache").join(&hash[..16]))
}

/// Check if a valid cache exists for the given parameters.
pub fn check_cache(video_path: &str, model: &str, lang: &str) -> Result<Option<PathBuf>> {
    let dir = cache_dir(video_path)?;
    let meta_path = dir.join("meta.json");
    let srt_path = dir.join("final.srt");

    if !meta_path.exists() || !srt_path.exists() {
        return Ok(None);
    }

    let meta_str = std::fs::read_to_string(&meta_path)
        .map_err(|e| AutoSubError::Cache(format!("Failed to read meta.json: {}", e)))?;

    let meta: CacheMeta = serde_json::from_str(&meta_str)
        .map_err(|e| AutoSubError::Cache(format!("Failed to parse meta.json: {}", e)))?;

    // Validate all fields match
    if meta.model == model
        && meta.lang == lang
        && meta.whisper_version == WHISPER_VERSION
        && meta.pipeline_version == PIPELINE_VERSION
        && meta.state == PipelineState::Completed
    {
        info!("Cache hit for {} (model={}, lang={})", video_path, model, lang);
        Ok(Some(srt_path))
    } else {
        debug!(
            "Cache miss: meta mismatch or incomplete (model: {} vs {}, lang: {} vs {}, state: {:?})",
            meta.model, model, meta.lang, lang, meta.state
        );
        Ok(None)
    }
}

/// Save raw whisper output to cache.
#[allow(dead_code)]
pub fn save_raw_json(video_path: &str, raw_json: &str) -> Result<PathBuf> {
    let dir = cache_dir(video_path)?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| AutoSubError::Cache(format!("Failed to create cache dir: {}", e)))?;

    let path = dir.join("raw.json");
    crate::utils::atomic_write(&path, raw_json)
        .map_err(|e| AutoSubError::Cache(format!("Failed to write raw.json: {}", e)))?;

    Ok(path)
}

/// Update only the pipeline state in meta.json.
pub fn update_state(
    video_path: &str,
    model: &str,
    lang: &str,
    duration: f32,
    state: PipelineState,
) -> Result<()> {
    let dir = cache_dir(video_path)?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| AutoSubError::Cache(format!("Failed to create cache dir: {}", e)))?;

    let meta = CacheMeta {
        model: model.to_string(),
        lang: lang.to_string(),
        duration,
        whisper_version: WHISPER_VERSION.to_string(),
        pipeline_version: PIPELINE_VERSION.to_string(),
        state,
    };

    let meta_json = serde_json::to_string_pretty(&meta)?;
    crate::utils::atomic_write(&dir.join("meta.json"), &meta_json)
        .map_err(|e| AutoSubError::Cache(format!("Failed to update meta.json: {}", e)))?;

    Ok(())
}

/// Save final SRT and metadata to cache.
pub fn save_final(
    video_path: &str,
    srt_content: &str,
    model: &str,
    lang: &str,
    duration: f32,
) -> Result<()> {
    let dir = cache_dir(video_path)?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| AutoSubError::Cache(format!("Failed to create cache dir: {}", e)))?;

    // Save SRT
    crate::utils::atomic_write(&dir.join("final.srt"), srt_content)
        .map_err(|e| AutoSubError::Cache(format!("Failed to write final.srt: {}", e)))?;

    // Save metadata with Completed state
    update_state(video_path, model, lang, duration, PipelineState::Completed)?;

    info!("Cache saved for {}", video_path);
    Ok(())
}

/// Compute SHA-256 hash of first 1MB of file (fast fingerprint).
fn compute_file_hash(path: &str) -> Result<String> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)
        .map_err(|e| AutoSubError::Cache(format!("Cannot open file for hashing: {}", e)))?;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024]; // 1MB
    let bytes_read = file
        .read(&mut buffer)
        .map_err(|e| AutoSubError::Cache(format!("Cannot read file for hashing: {}", e)))?;

    hasher.update(&buffer[..bytes_read]);
    // Also hash the file path and size for uniqueness
    hasher.update(path.as_bytes());
    if let Ok(metadata) = std::fs::metadata(path) {
        hasher.update(metadata.len().to_le_bytes());
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Clear all cached data.
pub fn clear_all_cache() -> Result<()> {
    let base = dirs::home_dir()
        .ok_or_else(|| AutoSubError::Cache("Cannot determine home directory".into()))?;
    let cache_root = base.join(".autosub").join("cache");
    if cache_root.exists() {
        fs::remove_dir_all(&cache_root)
            .map_err(|e| AutoSubError::Cache(format!("Failed to clear cache: {}", e)))?;
        info!("All cache cleared");
    }
    Ok(())
}
