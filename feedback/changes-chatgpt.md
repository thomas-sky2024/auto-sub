Kế hoạch này nhìn qua thì “có vẻ đơn giản”, nhưng thực ra đây là một **feature mở rộng rất lớn về mặt hệ thống** (không chỉ UI). Nếu làm không cẩn thận, nó có thể phá vỡ các giả định ổn định mà bạn đã xây dựng trong v5.

Mình sẽ review **rất thẳng + rất kỹ (production mindset)** 👇

---

# 🧠 1. Đánh giá tổng thể

## ✅ Ý tưởng: RẤT ĐÚNG

* Audio file support → gần như “free win”
* YouTube / URL → tăng giá trị sản phẩm mạnh
* Reuse pipeline → đúng hướng

---

## ⚠️ Nhưng implementation hiện tại: **chưa đủ an toàn**

Bạn đang giả định:

```text
URL → download → local path → pipeline
```

👉 Đây là **oversimplification**

---

# 🔥 2. Vấn đề lớn nhất: yt-dlp integration

## ❗ 2.1 Dependency problem (CỰC QUAN TRỌNG)

Bạn hỏi:

> bundle hay yêu cầu user cài?

👉 Trả lời dứt khoát:

> ❌ KHÔNG được phụ thuộc vào user cài
> ✅ PHẢI bundle

---

## Lý do:

* Tauri app → user non-tech
* PATH không ổn định
* version mismatch
* macOS sandbox → PATH có thể không đọc được

---

## ✅ Giải pháp chuẩn:

### bundle binary:

* `yt-dlp_macos`
* đặt trong:

```text
src-tauri/binaries/
```

---

### gọi như sidecar:

```rust
Command::new(resolve_path("yt-dlp"))
```

---

---

# ⚠️ 2.2 Security issue (RẤT NGUY HIỂM)

User input:

```text
url: String
```

👉 nếu không sanitize:

* command injection
* shell exploit

---

## ❌ sai:

```rust
Command::new("yt-dlp").arg(url)
```

---

## ✅ đúng:

```rust
if !url.starts_with("http") {
    return Err("Invalid URL");
}
```

* validate domain nếu cần

---

---

# ⚠️ 2.3 yt-dlp output không ổn định

👉 bạn đang assume:

```text
download → file path
```

👉 nhưng thực tế:

* yt-dlp có thể:

  * merge audio/video
  * rename file
  * output nhiều file

---

## ✅ Fix bắt buộc:

```bash
-o ~/.autosub/downloads/%(id)s.%(ext)s
```

---

👉 đảm bảo:

* predictable filename

---

---

# ⚡ 3. Kiến trúc nên thay đổi (QUAN TRỌNG)

## ❗ hiện tại (proposal):

```text
Frontend → download_url → pipeline
```

---

## ✅ nên chuyển thành:

```text
Source Resolver Layer (NEW)
    ├── Local file
    ├── Audio file
    └── URL (yt-dlp)
            ↓
        normalized local file
            ↓
        pipeline
```

---

## 👉 Tạo file mới:

```text
source.rs
```

---

## Ví dụ:

```rust
enum InputSource {
    Local(PathBuf),
    Url(String),
}
```

---

---

# 💾 4. Download system (thiếu nhiều thứ)

---

## ❗ thiếu:

### 1. cache download

👉 nếu user paste lại link:
→ không download lại

---

## ✅ fix:

```text
~/.autosub/downloads/
  youtube_<id>.m4a
```

---

---

### 2. cleanup

👉 nếu không:

* disk sẽ đầy

---

## ✅ fix:

* limit:

```text
max 5GB
```

---

---

### 3. progress

👉 download có thể rất lâu

---

## ✅ cần:

```text
Downloading... 35%
```

---

---

# ⚠️ 5. Format vấn đề (rất quan trọng)

---

## ❗ yt-dlp có thể trả:

* webm
* m4a
* opus

---

👉 ffmpeg của bạn phải handle tất cả

---

## ✅ fix:

ép format:

```bash
--extract-audio --audio-format wav
```

---

👉 hoặc:

```bash
--audio-format m4a
```

---

---

# ⚡ 6. Pipeline impact (rất quan trọng)

---

## ❗ audio file input

→ không cần ffmpeg extract video

---

## ✅ fix:

```rust
if is_audio_file(path) {
    skip extract stage
}
```

---

---

# 🧩 7. Code đề xuất (production-ready)

---

## 🆕 `source.rs`

```rust
pub async fn resolve_source(input: InputSource) -> Result<PathBuf, AutoSubError> {
    match input {
        InputSource::Local(path) => Ok(path),
        InputSource::Url(url) => download(url).await,
    }
}
```

---

---

## 🆕 `download.rs`

```rust
pub async fn download(url: String) -> Result<PathBuf, AutoSubError> {
    let output = format!("{}/%(id)s.%(ext)s", download_dir());

    let status = Command::new("yt-dlp")
        .args([
            "--extract-audio",
            "--audio-format", "wav",
            "-o", &output,
            &url
        ])
        .status();

    if !status.success() {
        return Err(AutoSubError::Download("yt-dlp failed".into()));
    }

    Ok(find_downloaded_file())
}
```

---

---

## 🆕 error.rs

```rust
#[error("Download failed: {0}")]
Download(String),
```

---

---

# 🖥️ 8. UI/UX cải tiến (quan trọng)

---

## ❗ hiện proposal:

* input URL → start

---

## ✅ nên làm:

### 2 mode:

```text
[ Local File ]
[ URL ]
```

---

---

## UX flow:

```text
Paste URL
→ Validate
→ Show title (optional)
→ Download
→ Transcribe
```

---

---

# ⚠️ 9. Pitfalls chưa được đề cập

---

## 🔥 1. YouTube rate limit

→ yt-dlp fail

---

## 🔥 2. Private / age-restricted video

→ fail silently

---

## 🔥 3. Network timeout

→ cần retry

---

## 🔥 4. File quá dài (3–5h video)

→ cần cảnh báo

---

---

# 🚀 10. Kiến trúc FINAL (sau khi nâng cấp)

```text
Frontend
   ↓
Source Resolver
   ├── Local video
   ├── Local audio
   └── URL (yt-dlp)
           ↓
      Download + cache
           ↓
Pipeline (unchanged)
```

---

# 🧾 Kết luận

👉 Feature này:

* ❌ không phải “small change”
* ✅ là **major architecture extension**

---

## Đánh giá proposal của bạn

| Tiêu chí             | Đánh giá   |
| -------------------- | ---------- |
| Ý tưởng              | ✅ rất tốt  |
| Thiết kế hiện tại    | ⚠️ chưa đủ |
| Production readiness | ❌ chưa đạt |

---

## Sau khi cải tiến (theo mình)

> 🔥 Feature này sẽ:

* tăng value app rất mạnh
* không phá architecture
* giữ được stability

---

# 👉 Khuyến nghị bước tiếp theo

## 🔧 Option A (khuyên mạnh)

👉 mình viết full:

* `source.rs`
* `download.rs`
* integrate vào pipeline

---

## ⚡ Option B

👉 viết luôn phần UI Svelte (URL + progress + error)

---

## 🧠 Option C

👉 thiết kế full download manager (queue + cache + retry)

---

Chọn 1 option, mình sẽ làm **code production-ready luôn (không pseudo)** 🚀
