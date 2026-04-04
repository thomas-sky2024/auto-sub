use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// ── Model Kind ────────────────────────────────────────────────────────────────

/// Loại model quyết định CLI args truyền vào sherpa-onnx-offline
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelKind {
    /// SenseVoice: --sense-voice + --tokens + --use-itn
    SenseVoice {
        model: PathBuf,  // model.int8.onnx
        tokens: PathBuf, // tokens.txt
    },
    /// FireRedASR CTC: --fire-red-asr-encoder + --fire-red-asr-decoder + --tokens
    FireRedAsr {
        encoder: PathBuf, // encoder.int8.onnx
        decoder: PathBuf, // decoder.int8.onnx
        tokens: PathBuf,  // tokens.txt
    },
    /// Paraformer: --paraformer + --tokens
    Paraformer {
        model: PathBuf,  // model.int8.onnx
        tokens: PathBuf, // tokens.txt
    },
}

impl ModelKind {
    /// Kiểm tra tất cả files có tồn tại và đủ lớn không
    pub fn files_valid(&self, min_bytes: u64) -> bool {
        match self {
            ModelKind::SenseVoice { model, tokens } => {
                file_ok(model, min_bytes) && file_ok(tokens, 100)
            }
            ModelKind::FireRedAsr { encoder, decoder, tokens } => {
                file_ok(encoder, min_bytes)
                    && file_ok(decoder, min_bytes / 4) // decoder nhỏ hơn encoder
                    && file_ok(tokens, 100)
            }
            ModelKind::Paraformer { model, tokens } => {
                file_ok(model, min_bytes) && file_ok(tokens, 100)
            }
        }
    }
}

fn file_ok(path: &PathBuf, min_bytes: u64) -> bool {
    fs::metadata(path)
        .map(|m| m.len() >= min_bytes)
        .unwrap_or(false)
}

// ── Model Config (runtime, resolved paths) ────────────────────────────────────

/// Config đầy đủ để chạy model (paths đã resolve)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub kind: ModelKind,
}

// ── Model Info (UI metadata, không cần model đã tải) ─────────────────────────

/// Thông tin hiển thị cho frontend — không cần model đã download
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub languages: Vec<String>,
    pub size_mb: u32,
    pub is_downloaded: bool,
    pub tier: u8, // 1=Fast, 2=Balanced, 3=Strong, 4=Best
}

// ── Model Manager ─────────────────────────────────────────────────────────────

pub struct ModelManager;

impl ModelManager {
    fn base_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".autosub")
            .join("models")
    }

    pub fn vad_model_path() -> PathBuf {
        Self::base_dir().join("silero_vad.onnx")
    }

    pub fn vad_ready() -> bool {
        file_ok(&Self::vad_model_path(), 1_000_000)
    }

    /// Resolve model ID → runtime config với đường dẫn tuyệt đối
    pub fn get_config(id: &str) -> Option<ModelConfig> {
        let base = Self::base_dir();

        match id {
            "sense-voice-2024" => {
                let dir = base.join("sense-voice-2024");
                Some(ModelConfig {
                    id: id.into(),
                    kind: ModelKind::SenseVoice {
                        model: dir.join("model.int8.onnx"),
                        tokens: dir.join("tokens.txt"),
                    },
                })
            }
            "sense-voice-2025" => {
                let dir = base.join("sense-voice-2025");
                // Thử int8 trước, fallback sang fp32
                let model = if dir.join("model.int8.onnx").exists() {
                    dir.join("model.int8.onnx")
                } else {
                    dir.join("model.onnx")
                };
                Some(ModelConfig {
                    id: id.into(),
                    kind: ModelKind::SenseVoice {
                        model,
                        tokens: dir.join("tokens.txt"),
                    },
                })
            }
            "paraformer-zh" => {
                let dir = base.join("paraformer-zh");
                Some(ModelConfig {
                    id: id.into(),
                    kind: ModelKind::Paraformer {
                        model: dir.join("model.int8.onnx"),
                        tokens: dir.join("tokens.txt"),
                    },
                })
            }
            "fire-red-v2" => {
                let dir = base.join("fire-red-v2");
                Some(ModelConfig {
                    id: id.into(),
                    kind: ModelKind::FireRedAsr {
                        encoder: dir.join("encoder.int8.onnx"),
                        decoder: dir.join("decoder.int8.onnx"),
                        tokens: dir.join("tokens.txt"),
                    },
                })
            }
            _ => {
                warn!("model_manager: unknown model id '{}'", id);
                None
            }
        }
    }

    /// Kiểm tra model đã tải đầy đủ và VAD sẵn sàng chưa
    pub fn is_ready(id: &str) -> bool {
        if !Self::vad_ready() {
            debug!("model_manager: VAD not ready for '{}'", id);
            return false;
        }

        let config = match Self::get_config(id) {
            Some(c) => c,
            None => return false,
        };

        // Min 5MB cho model chính
        let ok = config.kind.files_valid(5 * 1024 * 1024);
        debug!("model_manager: is_ready('{}') = {}", id, ok);
        ok
    }

    /// Trả về tất cả models (kèm trạng thái download) để hiển thị UI
    pub fn all_model_info() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "sense-voice-2024".into(),
                display_name: "SenseVoice Small 2024".into(),
                description: "~60MB · Nhanh nhất · zh/en/ja/ko/yue · Nhận diện cảm xúc".into(),
                languages: vec!["zh","yue","en","ja","ko"].iter().map(|s| s.to_string()).collect(),
                size_mb: 60,
                is_downloaded: Self::is_ready("sense-voice-2024"),
                tier: 1,
            },
            ModelInfo {
                id: "sense-voice-2025".into(),
                display_name: "SenseVoice Small 2025".into(),
                description: "~60MB · Nhanh · Cập nhật tháng 9/2025 · Chính xác hơn 2024".into(),
                languages: vec!["zh","yue","en","ja","ko"].iter().map(|s| s.to_string()).collect(),
                size_mb: 60,
                is_downloaded: Self::is_ready("sense-voice-2025"),
                tier: 1,
            },
            ModelInfo {
                id: "paraformer-zh".into(),
                display_name: "Paraformer-zh (Alibaba)".into(),
                description: "~220MB · Cân bằng · Chuyên tiếng Trung Phổ thông · Dấu câu tốt".into(),
                languages: vec!["zh"].iter().map(|s| s.to_string()).collect(),
                size_mb: 220,
                is_downloaded: Self::is_ready("paraformer-zh"),
                tier: 2,
            },
            ModelInfo {
                id: "fire-red-v2".into(),
                display_name: "FireRedASR v2 CTC".into(),
                description: "~250MB · Mạnh nhất · zh/en + 20 phương ngữ · Tốt nhất cho tiếng Trung".into(),
                languages: vec!["zh","en","yue","dialect"].iter().map(|s| s.to_string()).collect(),
                size_mb: 250,
                is_downloaded: Self::is_ready("fire-red-v2"),
                tier: 3,
            },
        ]
    }

    /// Liệt kê các model đã tải xong và sẵn sàng
    pub fn list_ready_ids() -> Vec<String> {
        Self::all_model_info()
            .into_iter()
            .filter(|m| m.is_downloaded)
            .map(|m| m.id)
            .collect()
    }

    pub fn get_models_dir() -> String {
        Self::base_dir().to_string_lossy().to_string()
    }
}
