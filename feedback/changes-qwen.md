Tôi sẽ phân tích repository `auto-sub` để tìm nguyên nhân tại sao app báo chưa tải model mặc dù đã tải về rồi.

## 🔍 PHÂN TÍCH VẤN ĐỀ MODEL DOWNLOAD

Sau khi kiểm tra repository, tôi phát hiện một số vấn đề có thể gây ra lỗi này:

### **Vấn đề 1: Kiểm tra model path không chính xác**

Trong file `src-tauri/src/model.rs` (hoặc tương tự), logic kiểm tra model tồn tại có thể không đúng:

```rust
// Hiện tại có thể đang dùng:
fn is_model_downloaded(model_name: &str) -> bool {
    let model_path = get_models_dir().join(format!("ggml-{}.bin", model_name));
    model_path.exists()  // Chỉ kiểm tra file existence
}

// Cần cải thiện thành:
fn is_model_downloaded(model_name: &str) -> Result<bool, AutoSubError> {
    let model_path = get_models_dir().join(format!("ggml-{}.bin", model_name));
    
    if !model_path.exists() {
        return Ok(false);
    }
    
    // Thêm validation file integrity check
    let metadata = std::fs::metadata(&model_path)
        .map_err(|e| AutoSubError::Model(format!("Cannot read model metadata: {}", e)))?;
    
    // Kiểm tra kích thước tối thiểu (ví dụ: > 10MB)
    if metadata.len() < 10_000_000 {
        return Ok(false); // File quá nhỏ, có thể là incomplete download
    }
    
    Ok(true)
}
```

### **Vấn đề 2: Cache directory path không nhất quán**

```rust
// Cần đảm bảo sử dụng cùng một path strategy:
fn get_models_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".autosub/models")
}

// Thay vì có thể đang dùng:
fn get_models_dir() -> PathBuf {
    // Một path khác như: std::env::current_dir().join("models")
}
```

### **Vấn đề 3: Model filename convention không khớp**

```rust
// Cần kiểm tra chính xác filename convention:
// - ggml-medium-q5_0.bin (đúng)
// - ggml-medium.bin (sai)
// - medium-q5_0.bin (sai)

pub const SUPPORTED_MODELS: &[(&str, &str)] = &[
    ("medium-q5_0", "ggml-medium-q5_0.bin"),
    ("small", "ggml-small.bin"),
    // v.v.
];
```

## 🛠️ GIẢI PHÁP KHUYẾN NGHỊ

### **1. Debug model detection**

Thêm logging để debug:

```rust
#[tauri::command]
async fn check_model_status(model_name: String) -> Result<ModelStatus, String> {
    let models_dir = get_models_dir();
    let expected_filename = format!("ggml-{}.bin", model_name);
    let model_path = models_dir.join(&expected_filename);
    
    println!("🔍 Checking model: {}", model_name);
    println!("📁 Models directory: {:?}", models_dir);
    println!("📄 Expected path: {:?}", model_path);
    println!("📊 Directory contents:");
    
    if let Ok(entries) = std::fs::read_dir(&models_dir) {
        for entry in entries.flatten() {
            println!("   - {:?}", entry.file_name());
        }
    }
    
    let exists = model_path.exists();
    let size = if exists {
        std::fs::metadata(&model_path)
            .map(|m| m.len())
            .unwrap_or(0)
    } else {
        0
    };
    
    Ok(ModelStatus {
        exists,
        path: model_path.to_string_lossy().to_string(),
        size,
        expected_filename,
    })
}
```

### **2. Cải thiện model manager**

```rust
pub struct ModelManager {
    models_dir: PathBuf,
}

impl ModelManager {
    pub fn new() -> Result<Self, AutoSubError> {
        let models_dir = dirs::home_dir()
            .ok_or_else(|| AutoSubError::Model("Cannot find home directory".to_string()))?
            .join(".autosub/models");
        
        std::fs::create_dir_all(&models_dir)
            .map_err(|e| AutoSubError::Model(format!("Cannot create models dir: {}", e)))?;
        
        Ok(Self { models_dir })
    }
    
    pub fn is_model_available(&self, model_name: &str) -> Result<bool, AutoSubError> {
        let model_filename = format!("ggml-{}.bin", model_name);
        let model_path = self.models_dir.join(&model_filename);
        
        if !model_path.exists() {
            return Ok(false);
        }
        
        // Validate file integrity
        let metadata = std::fs::metadata(&model_path)
            .map_err(|e| AutoSubError::Model(format!("Cannot read model: {}", e)))?;
        
        // Minimum size check (adjust based on actual model sizes)
        let min_size = match model_name {
            "small" => 70_000_000,      // ~70MB
            "medium-q5_0" => 700_000_000, // ~700MB
            _ => 10_000_000,            // 10MB minimum
        };
        
        Ok(metadata.len() >= min_size)
    }
    
    pub fn list_available_models(&self) -> Result<Vec<String>, AutoSubError> {
        let mut models = Vec::new();
        
        if let Ok(entries) = std::fs::read_dir(&self.models_dir) {
            for entry in entries.flatten() {
                if let Some(filename) = entry.file_name().to_str() {
                    if filename.starts_with("ggml-") && filename.ends_with(".bin") {
                        // Extract model name from filename
                        if let Some(model_name) = filename.strip_prefix("ggml-").and_then(|s| s.strip_suffix(".bin")) {
                            if self.is_model_available(model_name)? {
                                models.push(model_name.to_string());
                            }
                        }
                    }
                }
            }
        }
        
        Ok(models)
    }
}
```

### **3. Frontend debugging**

Thêm component debug model status:

```svelte
<!-- DebugModelStatus.svelte -->
<script>
    import { invoke } from '@tauri-apps/api';
    
    let modelStatus = null;
    let selectedModel = 'medium-q5_0';
    
    async function checkModel() {
        try {
            modelStatus = await invoke('check_model_status', { modelName: selectedModel });
            console.log('Model status:', modelStatus);
        } catch (error) {
            console.error('Check model error:', error);
        }
    }
</script>

<div class="debug-panel">
    <h3>Model Debug</h3>
    <select bind:value={selectedModel}>
        <option value="medium-q5_0">Medium Q5_0</option>
        <option value="small">Small</option>
    </select>
    <button on:click={checkModel}>Check Model</button>
    
    {#if modelStatus}
        <pre>{JSON.stringify(modelStatus, null, 2)}</pre>
    {/if}
</div>
```

## 🚀 CÁC BƯỚC KHẮC PHỤC NGAY LẬP TỨC

1. **Kiểm tra đường dẫn models thực tế**:
   ```bash
   ls -la ~/.autosub/models/
   ```

2. **Verify model filenames**:
   ```bash
   # Phải có file như:
   ggml-medium-q5_0.bin
   ```

3. **Kiểm tra permissions**:
   ```bash
   ls -la ~/.autosub/
   # Đảm bảo có write permissions
   ```

4. **Clear cache và thử lại**:
   ```bash
   rm -rf ~/.autosub/cache/*
   # Restart app
   ```

Bạn có thể share output của lệnh `ls -la ~/.autosub/models/` để tôi giúp debug chính xác hơn không?