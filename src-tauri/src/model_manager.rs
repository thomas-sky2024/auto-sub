use std::path::PathBuf;
use std::fs;
use log::{debug, warn};

pub struct ModelManager;

impl ModelManager {
    /// Returns the canonical absolute path to the models directory.
    /// Uses home_dir to ensure proper path expansion (no ~ tilde expand needed).
    fn models_directory() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let models_dir = home.join(".autosub").join("models");
        debug!("model_manager: models_directory -> {:?}", models_dir);
        models_dir
    }

    /// Returns the absolute path to a model file.
    /// Handles common naming variations for whisper models.
    /// Includes proper normalization and canonicalization.
    pub fn get_model_path(model_name: &str) -> PathBuf {
        let models_dir = Self::models_directory();

        // Normalize model name for filename
        let normalized = model_name.to_lowercase().replace(" ", "-");
        debug!("model_manager: get_model_path('{}') -> normalized: '{}'", model_name, normalized);

        // Try exact match first
        let exact_path = models_dir.join(format!("ggml-{}.bin", normalized));
        debug!("model_manager: checking exact match: {:?}", exact_path);
        if exact_path.exists() {
            debug!("model_manager: found exact match: {:?}", exact_path);
            return exact_path;
        }

        // Try with common suffixes if exact match fails
        let common_variants = vec![
            format!("ggml-{}.bin", normalized),
            format!("ggml-model-{}.bin", normalized),
            format!("{}.bin", normalized),
        ];

        for variant in common_variants {
            let p = models_dir.join(&variant);
            debug!("model_manager: checking variant: {:?}", p);
            if p.exists() {
                debug!("model_manager: found variant: {:?}", p);
                return p;
            }
        }

        // Default path if none exist yet
        debug!("model_manager: no model found, returning default: {:?}", exact_path);
        exact_path
    }

    /// Verifies if a model exists and has a minimum size.
    /// Includes proper logging for debugging path/permission issues.
    pub fn verify_model(model_name: &str) -> bool {
        debug!("model_manager: verify_model('{}') starting", model_name);

        let path = Self::get_model_path(model_name);

        if !path.exists() {
            debug!("model_manager: exact path not found, checking directory fallback: {:?}", path);

            // Also check for directory contents for matching patterns
            // specifically for "large-v3" which might be named "largev3"
            let models_dir = Self::models_directory();
            if let Ok(entries) = fs::read_dir(&models_dir) {
                let pattern = model_name.to_lowercase().replace("-", "").replace(" ", "");
                debug!("model_manager: searching directory for pattern: '{}'", pattern);

                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string().to_lowercase();
                    debug!("model_manager: checking directory entry: {}", name);

                    if name.contains(&pattern) && name.ends_with(".bin") {
                        // Found a likely match
                        if let Ok(meta) = fs::metadata(&entry_path) {
                            let size_mb = meta.len() / (1024 * 1024);
                            if meta.len() > 10 * 1024 * 1024 {
                                debug!("model_manager: fuzzy match found: {:?} ({}MB)", entry_path, size_mb);
                                return true;
                            } else {
                                warn!("model_manager: file too small ({} bytes): {:?}", meta.len(), entry_path);
                            }
                        } else {
                            warn!("model_manager: failed to read metadata for: {:?}", entry_path);
                        }
                    }
                }
            } else {
                warn!("model_manager: could not read models directory: {:?}", models_dir);
            }

            debug!("model_manager: model '{}' not found", model_name);
            return false;
        }

        // Check file size (must be at least 10MB to be a valid small model)
        if let Ok(metadata) = fs::metadata(&path) {
            let size_mb = metadata.len() / (1024 * 1024);
            if metadata.len() > 10 * 1024 * 1024 {
                debug!("model_manager: model verified: {:?} ({}MB)", path, size_mb);
                return true;
            } else {
                warn!("model_manager: model file too small: {:?} ({} bytes)", path, metadata.len());
            }
        } else {
            warn!("model_manager: failed to read metadata for: {:?}", path);
        }

        false
    }

    /// Returns a list of all verified models in the models directory.
    pub fn list_models() -> Vec<String> {
        let models_dir = Self::models_directory();
        debug!("model_manager: list_models from {:?}", models_dir);

        let mut models = Vec::new();

        if let Ok(entries) = fs::read_dir(&models_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("ggml-") && name.ends_with(".bin") {
                    let model_name = name
                        .trim_start_matches("ggml-")
                        .trim_end_matches(".bin")
                        .to_string();

                    if Self::verify_model(&model_name) {
                        debug!("model_manager: adding to list: {}", model_name);
                        models.push(model_name);
                    }
                }
            }
        } else {
            warn!("model_manager: could not read models directory: {:?}", models_dir);
        }

        debug!("model_manager: list_models found {} models", models.len());
        models
    }

    /// Returns the path to the Silero VAD model for voice activity detection.
    pub fn get_vad_model_path() -> PathBuf {
        let models_dir = Self::models_directory();
        let vad_path = models_dir.join("silero_vad2.onnx");
        debug!("model_manager: vad model path: {:?}", vad_path);
        vad_path
    }

    /// Checks if Silero VAD model exists and is valid.
    pub fn vad_model_ready() -> bool {
        let path = Self::get_vad_model_path();
        debug!("model_manager: checking VAD model: {:?}", path);

        if let Ok(metadata) = fs::metadata(&path) {
            // VAD model should be at least 40MB
            let size_mb = metadata.len() / (1024 * 1024);
            let is_valid = metadata.len() > 40 * 1024 * 1024;

            if is_valid {
                debug!("model_manager: VAD model valid ({}MB)", size_mb);
            } else {
                warn!("model_manager: VAD model too small ({} bytes, expected >40MB)", metadata.len());
            }

            return is_valid;
        } else {
            debug!("model_manager: VAD model not found at {:?}", path);
        }

        false
    }

    /// Returns the absolute directory where models should be placed.
    pub fn get_models_dir() -> String {
        let models_dir = Self::models_directory();
        models_dir.to_string_lossy().to_string()
    }

    /// Returns the path to the Demucs model for vocal separation.
    pub fn get_demucs_model_path() -> PathBuf {
        let models_dir = Self::models_directory();

        // Try common demucs model names
        let variants = vec![
            "ggml-model-htdemucs-4s-f16.bin",
            "ggml-model-htdemucs-4s.bin",
            "ggml-htdemucs-4s.bin",
            "ggml-demucs.bin",
            "demucs.bin",
        ];

        for variant in variants {
            let path = models_dir.join(variant);
            if path.exists() {
                debug!("model_manager: found demucs model: {:?}", path);
                return path;
            }
        }

        // Default fallback
        debug!("model_manager: no demucs model found, returning default");
        models_dir.join("ggml-model-htdemucs-4s-f16.bin")
    }

    /// Verifies if Demucs model exists and is valid.
    #[allow(dead_code)]
    pub fn demucs_model_ready() -> bool {
        let path = Self::get_demucs_model_path();
        debug!("model_manager: checking demucs model: {:?}", path);

        if let Ok(metadata) = fs::metadata(&path) {
            let size_mb = metadata.len() / (1024 * 1024);
            let is_valid = metadata.len() > 50 * 1024 * 1024; // Demucs models are typically 80MB+

            if is_valid {
                debug!("model_manager: demucs model valid ({}MB)", size_mb);
            } else {
                warn!("model_manager: demucs model too small ({} bytes, expected >50MB)", metadata.len());
            }

            return is_valid;
        } else {
            debug!("model_manager: demucs model not found at {:?}", path);
        }

        false
    }
}
