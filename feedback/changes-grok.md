**Nhận xét tổng quát về bản kế hoạch thêm tính năng "Support for Audio Files & YouTube Sources"**:

Bản kế hoạch này **tốt và hợp lý** ở mức cơ bản, phù hợp với hướng phát triển hiện tại của AutoSub (Tauri + Rust pipeline). Nó giữ được sự đơn giản (chỉ thêm 1 command `download_url` và 1 stage "Downloading"), tận dụng được pipeline hiện có sau khi có file local.

Tuy nhiên, vẫn còn **một số điểm yếu quan trọng** cần cải tiến để phù hợp với triết lý của dự án (native, nhẹ, ít dependency hệ thống, dễ distribute .dmg, chạy ổn trên Intel 2019).

### Điểm mạnh
- Logic rõ ràng: URL → download → local path → pipeline bình thường.
- Hỗ trợ audio files (mp3, wav, m4a...) là cần thiết và dễ làm (ffmpeg đã xử lý tốt).
- Thêm stage "Downloading" vào progress bar là UX tốt.
- Có xem xét error handling và verification (dù manual).

### Điểm yếu & Rủi ro lớn
1. **yt-dlp dependency** — Đây là vấn đề nghiêm trọng nhất trong kế hoạch hiện tại.
   - Nếu chỉ kiểm tra PATH và yêu cầu user cài thủ công (`brew install yt-dlp`), app sẽ **không phải "native single .dmg"** nữa. Nhiều user (đặc biệt người Việt hoặc người mới) sẽ bỏ cuộc khi thấy "cần cài thêm tool".
   - yt-dlp thường update thường xuyên để bypass YouTube thay đổi → dễ break.

2. **Sidecar chưa được tận dụng** — Dự án đang dùng sidecar cho `ffmpeg` và `whisper-main`. Nên **tiếp tục phong cách này** với yt-dlp để giữ tính nhất quán và "zero extra install".

3. **Không xử lý tốt các edge case**:
   - Video dài / playlist / age-restricted / membership content.
   - Download chỉ audio (không cần video để tiết kiệm thời gian + dung lượng).
   - Tên file download (sanitize để tránh lỗi path).
   - Progress download (yt-dlp có output progress tốt, nên parse và emit event).

4. **Frontend**: Chỉ thêm 1 input field là chưa đủ UX. Nên có tab hoặc toggle rõ ràng giữa "Local File" và "URL Source".

### Khuyến nghị cải tiến (Phiên bản nâng cấp)

**Chiến lược khuyến nghị mạnh mẽ nhất (Production-grade)**:

- **Bundled yt-dlp as sidecar** (giống ffmpeg và whisper-main).
  - yt-dlp cung cấp **standalone executable** cho macOS (universal binary: yt-dlp_macos).
  - Download từ GitHub releases và đặt vào `src-tauri/binaries/yt-dlp-x86_64-apple-darwin` (hoặc universal).
  - Thêm vào `tauri.conf.json` trong `externalBin`.
  - Rust gọi qua `tauri_plugin_shell::ShellExt::sidecar("yt-dlp")`.

- **Ưu tiên chỉ download audio** (bestaudio) để nhanh và tiết kiệm dung lượng.
  - Command mẫu: `yt-dlp -f bestaudio --extract-audio --audio-format mp3 -o "%(title)s.%(ext)s" <url>`

- **Thêm thư mục cache riêng**: `~/.autosub/downloads/` hoặc tích hợp luôn vào cache system hiện có (dùng SHA hash của URL làm key).

- **Progress handling**: Parse stdout của yt-dlp (nó có dòng `[download] ... %`).

- **Error handling tốt hơn**: 
  - yt-dlp không có → thông báo "Cannot download from this URL" + gợi ý kiểm tra link.
  - Network fail, age gate, etc.

### Cải tiến cụ thể cho từng phần

**Backend (Rust) – Nâng cấp:**
- Thêm command: `download_from_url(url: String) -> Result<String, AutoSubError>` (trả về local path).
- Trong `error.rs`: thêm variant `Download(String)`.
- Tạo file mới `src-tauri/src/downloader.rs` (tách biệt, dễ maintain).
- Kiểm tra yt-dlp sidecar tồn tại, nếu không → graceful error + hướng dẫn (nhưng vì bundled nên hiếm xảy ra).
- Sanitize filename trước khi lưu.

**Frontend (Svelte) – Nâng cấp:**
- Thay vì chỉ 1 input field, nên có **radio/tab**: 
  - Local File (drag-drop + picker)
  - From URL (input text + button "Download & Transcribe")
- Disable nút Start nếu cả hai đều rỗng.
- Thêm stage "Downloading from URL..." với progress % nếu parse được.
- Hiển thị tên file sau khi download xong.

**Tauri config:**
- Thêm yt-dlp vào `externalBin`.
- Cập nhật capability để cho phép sidecar execute.

**Về Open Question trong kế hoạch:**
> **yt-dlp dependency**: Nên **bundle** (sidecar) thay vì yêu cầu user cài thủ công.  
Lý do: Giữ được trải nghiệm "double-click .dmg → chạy ngay".  
Nếu bundle làm .dmg to hơn ~5-10 MB thì vẫn chấp nhận được (yt-dlp standalone chỉ vài MB).

### Kế hoạch thực hiện đề xuất (ngắn gọn)

1. Download yt-dlp_macos standalone từ GitHub releases → rename và đặt vào `src-tauri/binaries/`.
2. Cập nhật `tauri.conf.json` + build script (tương tự build-whisper.sh).
3. Tạo `downloader.rs` với command `download_from_url`.
4. Cập nhật pipeline để nhận cả local path (từ file picker hoặc từ downloader).
5. Cập nhật UI Svelte với toggle nguồn input.
6. Test: mp3 local, YouTube link (audio-only), invalid URL.

Bạn muốn tôi **viết code cụ thể** cho phần nào trước không?

- `downloader.rs` + command `download_from_url`
- Cập nhật `error.rs` + `pipeline.rs`
- Frontend Svelte changes (`+page.svelte` hoặc components)
- Cập nhật `tauri.conf.json` + build script

Hoặc bạn muốn tôi viết **toàn bộ đề xuất cải tiến** dưới dạng file markdown mới (v5.2 plan)?

Cứ nói rõ, tôi sẽ cung cấp code chi tiết ngay.