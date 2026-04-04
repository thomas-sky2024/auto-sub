#!/bin/bash
set -e

# This script downloads sidecar binaries for AutoSub
# Usage: ./src-tauri/setup-binaries.sh

BIN_DIR="src-tauri/binaries"
mkdir -p "$BIN_DIR"

echo "--- Setting up sidecar binaries ---"

# yt-dlp setup (macOS Universal)
ARCH=$(uname -m)
if [ "$ARCH" == "arm64" ]; then
    ARCH="aarch64"
fi

YTDLP_PATH=$(which yt-dlp || echo "")

if [ -n "$YTDLP_PATH" ]; then
    echo "Found yt-dlp in system at $YTDLP_PATH. Copying to sidecar directory..."
    cp "$YTDLP_PATH" "$BIN_DIR/yt-dlp-aarch64-apple-darwin"
    cp "$YTDLP_PATH" "$BIN_DIR/yt-dlp-x86_64-apple-darwin"
else
    echo "Downloading yt-dlp (macOS Universal)..."
    curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos -o "$BIN_DIR/yt-dlp-aarch64-apple-darwin"
    cp "$BIN_DIR/yt-dlp-aarch64-apple-darwin" "$BIN_DIR/yt-dlp-x86_64-apple-darwin"
fi

chmod +x "$BIN_DIR/yt-dlp-aarch64-apple-darwin"
chmod +x "$BIN_DIR/yt-dlp-x86_64-apple-darwin"

# Ensure sherpa-onnx (vad) exists (built via build-sensevoice.sh)
if [ ! -f "$BIN_DIR/sherpa-onnx-vad-aarch64-apple-darwin" ] && [ ! -f "$BIN_DIR/sherpa-onnx-vad-x86_64-apple-darwin" ]; then
    echo "WARNING: sherpa-onnx-vad binary missing. Please run build-scripts/build-sensevoice.sh"
fi

echo "Sidecar binaries setup complete."
