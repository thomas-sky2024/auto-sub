use crate::error::{AutoSubError, Result};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const PIPELINE_VERSION: &str = "v5";
const WHISPER_VERSION: &str = "1.8.4";

/// Cache metadata for validation.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CacheMeta {
    pub model: String,
    pub lang: String,
    pub duration: f32,
    pub whisper_version: String,
    pub pipeline_version: String,
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
    {
        info!("Cache hit for {} (model={}, lang={})", video_path, model, lang);
        Ok(Some(srt_path))
    } else {
        debug!(
            "Cache miss: meta mismatch (model: {} vs {}, lang: {} vs {}, whisper: {} vs {}, pipeline: {} vs {})",
            meta.model, model, meta.lang, lang, meta.whisper_version, WHISPER_VERSION, meta.pipeline_version, PIPELINE_VERSION
        );
        Ok(None)
    }
}

/// Save raw whisper output to cache.
pub fn save_raw_json(video_path: &str, raw_json: &str) -> Result<PathBuf> {
    let dir = cache_dir(video_path)?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| AutoSubError::Cache(format!("Failed to create cache dir: {}", e)))?;

    let path = dir.join("raw.json");
    std::fs::write(&path, raw_json)
        .map_err(|e| AutoSubError::Cache(format!("Failed to write raw.json: {}", e)))?;

    Ok(path)
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
    std::fs::write(dir.join("final.srt"), srt_content)
        .map_err(|e| AutoSubError::Cache(format!("Failed to write final.srt: {}", e)))?;

    // Save metadata
    let meta = CacheMeta {
        model: model.to_string(),
        lang: lang.to_string(),
        duration,
        whisper_version: WHISPER_VERSION.to_string(),
        pipeline_version: PIPELINE_VERSION.to_string(),
    };

    let meta_json = serde_json::to_string_pretty(&meta)?;
    std::fs::write(dir.join("meta.json"), meta_json)
        .map_err(|e| AutoSubError::Cache(format!("Failed to write meta.json: {}", e)))?;

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
