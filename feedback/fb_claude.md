GitHub đang block direct tree URLs. Mình sẽ thử fetch raw files.GitHub chỉ cho phép đọc README qua `blob/` URL, không cho phép đọc source files trực tiếp. Nhưng từ kiến trúc repo (Tauri v2 + whisper.cpp **CLI Sidecar**) và lỗi bạn mô tả, mình có thể phân tích rõ vấn đề.

---

## 🔍 Chẩn đoán — Lỗi "chưa tải model" mặc dù đã tải

### Sự khác biệt quan trọng với phân tích của Qwen

Qwen phân tích theo hướng **thư viện Rust** (`model.rs`, `#[tauri::command]`). Nhưng `auto-sub` dùng **whisper.cpp CLI Sidecar** — đây là kiến trúc khác hoàn toàn. Lỗi rất có thể **không nằm ở logic kiểm tra**, mà nằm ở **path mismatch** giữa nơi bạn đặt model và nơi app tìm.

---

### Nguyên nhân 1 — Path khác nhau giữa `setup-models.sh` và runtime (khả năng cao nhất)

`setup-models.sh` tải model về `src-tauri/binaries/models/` hoặc path tương đối trong source tree. Nhưng khi app chạy (dev hay build), Tauri sidecar tìm model ở path **tuyệt đối** khác, thường là:

```
~/Library/Application Support/auto-sub/models/   ← macOS app data
# hoặc
~/.autosub/models/
```

Kiểm tra ngay bằng lệnh:

```bash
# Xem file model của bạn thực sự đang ở đâu:
find ~ -name "ggml-*.bin" 2>/dev/null

# Xem app đang tìm ở đâu (check app data dir macOS):
ls ~/Library/Application\ Support/auto-sub/
ls ~/.autosub/models/ 2>/dev/null
```

---

### Nguyên nhân 2 — Tên file không khớp

whisper.cpp CLI sidecar expect tên file theo convention cụ thể. So sánh:

```bash
ls -lh ~/.autosub/models/
# hoặc
ls -lh ~/Library/Application\ Support/auto-sub/models/
```

File phải đúng tên. Ví dụ nếu app expect `ggml-medium-q5_0.bin` nhưng file thực tế là `ggml-medium.bin` → báo chưa tải.

---

### Nguyên nhân 3 — Sidecar nhận path không đúng lúc gọi

Với kiến trúc sidecar, Rust code gọi whisper như:
```rust
Command::new("whisper-sidecar")
    .args(["-m", &model_path, "-f", &audio_path])
```

Nếu `model_path` được resolve sai (relative path trong dev mode vs absolute path trong build), lệnh sẽ fail hoặc app báo "chưa tải" vì sidecar return error.

---

## 🛠️ Cách fix nhanh nhất — Chạy ngay

**Bước 1**: Tìm model đang ở đâu:
```bash
find ~ -name "ggml-*.bin" 2>/dev/null -ls
```

**Bước 2**: Tìm app đang tìm ở đâu — mở Console.app, lọc theo `auto-sub`, chạy app và xem log. Hoặc thêm tạm vào terminal:
```bash
pnpm tauri dev 2>&1 | grep -i "model\|path\|ggml"
```

**Bước 3**: Nếu path lệch, tạo symlink tạm để test:
```bash
mkdir -p ~/Library/Application\ Support/auto-sub/models
ln -s /đường/dẫn/thực/ggml-medium-q5_0.bin \
      ~/Library/Application\ Support/auto-sub/models/ggml-medium-q5_0.bin
```

---

## 📌 Điểm bất đồng với Qwen

Phân tích của Qwen đúng về **hướng fix** (kiểm tra path, validate file size) nhưng đề xuất thêm `#[tauri::command] pub async fn check_model_availability` là **không cần thiết** vì đây là sidecar app — model path được pass thẳng vào CLI binary, không qua command riêng.

Fix thực tế cần làm là: **thống nhất 1 path duy nhất** giữa `setup-models.sh` và runtime Rust code. Bạn có thể share output của lệnh `find` ở trên để mình xác định chính xác hơn không?