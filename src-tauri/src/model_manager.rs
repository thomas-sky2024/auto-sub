use std::path::PathBuf;
use std::fs;


pub struct ModelManager;

impl ModelManager {
    /// Returns the absolute path to a model file.
    pub fn get_model_path(model_name: &str) -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".autosub")
            .join("models")
            .join(format!("ggml-{}.bin", model_name))
    }

    /// Verifies if a model exists and has a minimum size.
    pub fn verify_model(model_name: &str) -> bool {
        let path = Self::get_model_path(model_name);
        if !path.exists() {
            return false;
        }

        // Check file size (must be at least 10MB to be a valid small model)
        if let Ok(metadata) = fs::metadata(&path) {
            return metadata.len() > 10 * 1024 * 1024;
        }

        false
    }

    /// Returns a list of all verified models in the models directory.
    pub fn list_models() -> Vec<String> {
        let home = dirs::home_dir().unwrap_or_default();
        let models_dir = home.join(".autosub").join("models");
        let mut models = Vec::new();

        if let Ok(entries) = fs::read_dir(models_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("ggml-") && name.ends_with(".bin") {
                    let model_name = name
                        .trim_start_matches("ggml-")
                        .trim_end_matches(".bin")
                        .to_string();
                    
                    if Self::verify_model(&model_name) {
                        models.push(model_name);
                    }
                }
            }
        }
        models
    }

    /// Returns the absolute directory where models should be placed.
    pub fn get_models_dir() -> String {
        let home = dirs::home_dir().unwrap_or_default();
        home.join(".autosub")
            .join("models")
            .to_string_lossy()
            .to_string()
    }
}
