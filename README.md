# 🎬 AutoSub — Native macOS Subtitle Generator

AutoSub is a production-grade, high-performance desktop application for generating subtitles from video files. Built specifically for **Intel Core i9 MacBook Pro (2019)** hardware using **Tauri v2**, **Rust**, and **SenseVoice-Small** (via `sherpa-onnx`), it provides a seamless, streaming-first experience with professional-grade accuracy.

![AutoSub UI Placeholder](https://via.placeholder.com/1200x800.png?text=AutoSub+Premium+Svelte+UI)

## 🔥 Key Features

- **5-Stage Fault-Tolerant Pipeline**: 
    - `FFmpeg Extraction` (Streaming audio directly via pipe)
    - `SenseVoice transcription` (SenseVoice-Small via sherpa-onnx with VAD)
    - `Validation Layer` (Fixing overlaps, dropping high-CPS errors)
    - `Pro-Grade Post-Processing` (CJK-aware, context merging, speaker pause detection)
    - `Multi-Format Export` (SRT/TXT with live preview)
- **Intel i9 Optimization**:
    - Build flags tuned for Coffee Lake architecture.
    - **Adaptive Thermal Control**: Dynamically adjusts thread count based on real-time CPU load and thermal state.
    - **Balanced/MaxSpeed Modes**: Toggle between 4 and 8-core parallel processing (SenseVoice is optimized for multi-threading).
- **Commercial-Grade Subtitles**:
    - **CPS Control**: Automatically splits fast-speech segments for comfortable reading.
    - **CJK-Aware**: Handles Chinese/Japanese/Korean without whitespace-splitting bugs, using unicode grapheme boundaries.
    - **Contextual Merging**: Intelligently merges short segments if they form a single sentence.
- **Smart Caching**: 4-layer cache system with SHA-256 fingerprinting and `meta.json` versioning to avoid re-procesing.

## 🛠️ Tech Stack

- **Framework**: [Tauri v2.1.0](https://tauri.app/)
- **Backend**: [Rust](https://www.rust-lang.org/)
- **Audio Engine**: [SenseVoice-Small](https://github.com/FunAudioLLM/SenseVoice) + [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)
- **Frontend**: [Svelte v5](https://svelte.dev/) + [Vite](https://vite.dev/)
- **Styling**: Modern CSS with Glassmorphism and Stage-aware UI.

## 🚀 Getting Started

### Prerequisites

- macOS (Intel or Apple Silicon)
- `pnpm`, `cargo`
- `ffmpeg` installed on system (for development)

### Installation

1. **Clone the repository**:
   ```bash
   git clone https://github.com/your-username/auto-sub.git
   cd auto-sub
   ```

2. **Download sherpa-onnx sidecar**:
   ```bash
   chmod +x build-scripts/*.sh
   ./build-scripts/build-sensevoice.sh
   ```

3. **Setup SenseVoice models**:
   ```bash
   ./build-scripts/setup-models.sh
   ```

4. **Install dependencies and run**:
   ```bash
   pnpm install
   pnpm tauri dev
   ```

## 🛠️ Troubleshooting

### Port 1420 already in use
If you see an error that port 1420 is in use, run:
```bash
lsof -ti:1420 | xargs kill -9
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
