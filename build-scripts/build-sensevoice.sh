#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN_DEST="$REPO_DIR/src-tauri/binaries"
mkdir -p "$BIN_DEST"

ARCH=$(uname -m)
if [ "$ARCH" = "arm64" ]; then
    TARGET_TRIPLE="aarch64-apple-darwin"
    SHERPA_ARCH="arm64"
else
    TARGET_TRIPLE="x86_64-apple-darwin"
    SHERPA_ARCH="x86_64"
fi

# Tìm version mới nhất tại: https://github.com/k2-fsa/sherpa-onnx/releases
SHERPA_VERSION="1.11.3"
TARBALL="sherpa-onnx-v${SHERPA_VERSION}-osx-universal2-static.tar.bz2"
URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/v${SHERPA_VERSION}/${TARBALL}"

echo "⬇️  Downloading sherpa-onnx v${SHERPA_VERSION} (universal2-static)..."
TMPDIR=$(mktemp -d)
curl -L "$URL" -o "$TMPDIR/$TARBALL" --progress-bar

echo "📦  Extracting..."
tar -xjf "$TMPDIR/$TARBALL" -C "$TMPDIR"

# Copy binary với đúng tên Tauri sidecar
SHERPA_BIN=$(find "$TMPDIR" -name "sherpa-onnx-offline" -type f | head -1)
if [ -z "$SHERPA_BIN" ]; then
    echo "❌ Không tìm thấy sherpa-onnx-offline binary"
    exit 1
fi

cp "$SHERPA_BIN" "$BIN_DEST/sherpa-onnx-${TARGET_TRIPLE}"
chmod +x "$BIN_DEST/sherpa-onnx-${TARGET_TRIPLE}"

rm -rf "$TMPDIR"

echo "✅ sherpa-onnx binary → $BIN_DEST/sherpa-onnx-${TARGET_TRIPLE}"
file "$BIN_DEST/sherpa-onnx-${TARGET_TRIPLE}"
