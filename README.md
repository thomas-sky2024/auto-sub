# 🎬 AutoSub — Native macOS Subtitle Generator

AutoSub is a production-grade, high-performance desktop application for generating subtitles from video files. Built specifically for **Intel Core i9 MacBook Pro (2019)** hardware using **Tauri v2**, **Rust**, and **whisper.cpp**, it provides a seamless, streaming-first experience with professional-grade accuracy.

![AutoSub UI Placeholder](https://via.placeholder.com/1200x800.png?text=AutoSub+Premium+Svelte+UI)

## 🔥 Key Features

- **5-Stage Fault-Tolerant Pipeline**: 
    - `FFmpeg Extraction` (Streaming audio directly via pipe)
    - `Whisper Decoding` (whisper.cpp with VAD for silence suppression)
    - `Validation Layer` (Fixing overlaps, dropping high-CPS errors)
    - `Pro-Grade Post-Processing` (CJK-aware, context merging, speaker pause detection)
    - `Multi-Format Export` (SRT/TXT with live preview)
- **Intel i9 Optimization**:
    - Build flags tuned for Coffee Lake architecture (`AVX512=OFF` to prevent crashes).
    - **Adaptive Thermal Control**: Dynamically adjusts thread count based on real-time CPU load and thermal state.
    - **Balanced/MaxSpeed Modes**: Toggle between 8 and 12-core parallel processing.
- **Commercial-Grade Subtitles**:
    - **CPS Control**: Automatically splits fast-speech segments for comfortable reading.
    - **CJK-Aware**: Handles Chinese/Japanese/Korean without whitespace-splitting bugs, using unicode grapheme boundaries.
    - **Contextual Merging**: Intelligently merges short segments if they form a single sentence.
- **Smart Caching**: 4-layer cache system with SHA-256 fingerprinting and `meta.json` versioning to avoid re-procesing.

## 🛠️ Tech Stack

- **Framework**: [Tauri v2.1.0](https://tauri.app/)
- **Backend**: [Rust](https://www.rust-lang.org/)
- **Audio Engine**: [whisper.cpp](https://github.com/ggerganov/whisper.cpp) (v1.8.4 CLI Sidecar)
- **Frontend**: [Svelte v5](https://svelte.dev/) + [Vite](https://vite.dev/)
- **Styling**: Modern CSS with Glassmorphism and Stage-aware UI.

## 🚀 Getting Started

### Prerequisites

- macOS (Intel or Apple Silicon)
- `pnpm`, `cargo`, `cmake`
- `ffmpeg` installed on system (for development)

### Installation

1. **Clone the repository**:
   ```bash
   git clone https://github.com/your-username/auto-sub.git
   cd auto-sub
   ```

2. **Build whisper.cpp sidecar**:
   ```bash
   chmod +x build-scripts/build-whisper.sh
   ./build-scripts/build-whisper.sh
   ```

3. **Install dependencies and run**:
   ```bash
   pnpm install
   pnpm tauri dev
   ```

## 📦 Production Distribution

To build the notarized DMG for Intel Mac:

1. **Copy FFmpeg binary** to `src-tauri/binaries/ffmpeg-x86_64-apple-darwin`.
2. **Run Turi Build**:
   ```bash
   pnpm tauri build --target x86_64-apple-darwin
   ```

## ⚠️ Intel Mac 2019 Pitfalls

AutoSub includes mandatory fixes for common Intel i9 issues:
1. **AVX-512 Crash**: Fixed by compiling whisper.cpp with `GGML_AVX512=OFF`.
2. **Thermal Throttling**: Pipeline is non-blocking; Rust `thermal.rs` monitors `sysinfo` to drop threads if temp/load peaks.
3. **Pipe Deadlocks**: Streaming raw PCM via `ffmpeg pipe:2` avoids large temp files and handles buffer pressure.

---
Built with ❤️ for professional subtitle editors.
