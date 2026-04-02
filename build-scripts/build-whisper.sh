#!/bin/bash
set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
WHISPER_DIR="$REPO_DIR/build-scripts/whisper.cpp"
ARCH=$(uname -m)

# Determine target triple and Metal support based on ACTUAL architecture
if [ "$ARCH" = "arm64" ]; then
    TARGET_TRIPLE="aarch64-apple-darwin"
    # Apple Silicon → enable Metal for GPU acceleration
    METAL_FLAG="-DGGML_METAL=ON"
    AVX_FLAGS="-DWHISPER_AVX=OFF -DWHISPER_AVX2=OFF -DWHISPER_AVX512=OFF"
    echo "✅ Detected Apple Silicon (arm64) — building with Metal support"
else
    TARGET_TRIPLE="x86_64-apple-darwin"
    # Intel → disable Metal, enable AVX but NOT AVX512 (crashes on Coffee Lake i9)
    METAL_FLAG="-DGGML_METAL=OFF"
    AVX_FLAGS="-DWHISPER_AVX=ON -DWHISPER_AVX2=ON -DWHISPER_AVX512=OFF"
    echo "✅ Detected Intel x86_64 — building CPU-only (Metal off, AVX512 off)"
fi

# Override for cross-compilation: Intel target from Apple Silicon host
if [ "${CROSS_COMPILE_X86:-0}" = "1" ] && [ "$ARCH" = "arm64" ]; then
    TARGET_TRIPLE="x86_64-apple-darwin"
    METAL_FLAG="-DGGML_METAL=OFF"
    AVX_FLAGS="-DWHISPER_AVX=ON -DWHISPER_AVX2=ON -DWHISPER_AVX512=OFF"
    CMAKE_EXTRA="-DCMAKE_OSX_ARCHITECTURES=x86_64"
    echo "🔧 Cross-compiling for x86_64 (Intel) from Apple Silicon"
else
    CMAKE_EXTRA=""
fi

BINARY_DEST="$REPO_DIR/src-tauri/binaries/whisper-main-${TARGET_TRIPLE}"

echo ""
echo "Build config:"
echo "  Target triple : $TARGET_TRIPLE"
echo "  Metal         : $METAL_FLAG"
echo "  AVX flags     : $AVX_FLAGS"
echo "  Output        : $BINARY_DEST"
echo ""

echo "Step 1: Cloning whisper.cpp..."
if [ ! -d "$WHISPER_DIR" ]; then
    git clone https://github.com/ggerganov/whisper.cpp "$WHISPER_DIR" --depth 1
else
    echo "   whisper.cpp already cloned, using existing copy"
fi

echo "Step 2: Configuring CMake..."
cd "$WHISPER_DIR"
rm -rf build && mkdir -p build && cd build

cmake .. \
  $AVX_FLAGS \
  $METAL_FLAG \
  -DWHISPER_BUILD_EXAMPLES=ON \
  -DCMAKE_BUILD_TYPE=Release \
  $CMAKE_EXTRA

echo "Step 3: Building (using $(sysctl -n hw.ncpu) cores)..."
make -j$(sysctl -n hw.ncpu)

echo "Step 4: Copying binary..."
mkdir -p "$(dirname "$BINARY_DEST")"

# whisper.cpp output binary name has changed across versions — try all known locations
if [ -f "bin/whisper-cli" ]; then
    cp bin/whisper-cli "$BINARY_DEST"
elif [ -f "bin/main" ]; then
    cp bin/main "$BINARY_DEST"
elif [ -f "examples/cli/whisper-cli" ]; then
    cp examples/cli/whisper-cli "$BINARY_DEST"
elif [ -f "examples/main/main" ]; then
    cp examples/main/main "$BINARY_DEST"
else
    echo "❌ Could not find whisper binary. Available files in bin/:"
    ls bin/ 2>/dev/null || echo "   (bin/ directory not found)"
    echo "Available files in examples/:"
    ls examples/ 2>/dev/null || echo "   (examples/ directory not found)"
    exit 1
fi

chmod +x "$BINARY_DEST"

echo ""
echo "✅ whisper-main built → $BINARY_DEST"
file "$BINARY_DEST"