use thiserror::Error;

/// Typed errors for each pipeline stage.
#[derive(Error, Debug)]
pub enum AutoSubError {
    #[error("Audio extraction failed: {0}")]
    AudioExtract(String),

    #[error("Whisper decode failed: {0}")]
    WhisperDecode(String),

    #[error("JSON parse failed: {0}")]
    ParseFailed(String),

    #[error("Validation failed: {0}")]
    Validation(String),

    #[error("Post-process failed: {0}")]
    PostProcess(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Export error: {0}")]
    Export(String),

    #[error("Job cancelled")]
    Cancelled,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

// Allow serialization to Tauri frontend
impl serde::Serialize for AutoSubError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AutoSubError>;
