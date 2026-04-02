#!/bin/bash
set -e

# Configuration
REPO_DIR=$(pwd)
WHISPER_DIR="$REPO_DIR/build-scripts/whisper.cpp"
ARCH=$(uname -m)

# Determine target triple based on host architecture
if [ "$ARCH" = "arm64" ]; then
    TARGET_TRIPLE="aarch64-apple-darwin"
    echo "⚠️  Detected Apple Silicon – building for ARM64"
    echo "   For Intel cross-compilation, set: export CROSS_COMPILE_X86=1"
else
    TARGET_TRIPLE="x86_64-apple-darwin"
fi

# Override for cross-compilation
if [ "${CROSS_COMPILE_X86:-0}" = "1" ] && [ "$ARCH" = "arm64" ]; then
    TARGET_TRIPLE="x86_64-apple-darwin"
    CMAKE_EXTRA="-DCMAKE_OSX_ARCHITECTURES=x86_64"
    echo "🔧 Cross-compiling for x86_64 (Intel)"
else
    CMAKE_EXTRA=""
fi

BINARY_DEST="$REPO_DIR/src-tauri/binaries/whisper-main-${TARGET_TRIPLE}"

echo "Step 1: Cloning whisper.cpp..."
if [ ! -d "$WHISPER_DIR" ]; then
    git clone https://github.com/ggerganov/whisper.cpp "$WHISPER_DIR" --depth 1
fi

echo "Step 2: Configuring CMake (AVX512=OFF for Intel safety)..."
cd "$WHISPER_DIR"
rm -rf build && mkdir -p build && cd build

cmake .. \
  -DWHISPER_AVX=ON \
  -DWHISPER_AVX2=ON \
  -DWHISPER_AVX512=OFF \
  -DWHISPER_METAL=OFF \
  -DWHISPER_BUILD_EXAMPLES=ON \
  -DCMAKE_BUILD_TYPE=Release \
  $CMAKE_EXTRA

echo "Step 3: Building..."
make -j$(sysctl -n hw.ncpu)

echo "Step 4: Copying binary..."
# Find the CLI binary (name varies by version)
if [ -f "bin/whisper-cli" ]; then
    cp bin/whisper-cli "$BINARY_DEST"
elif [ -f "bin/main" ]; then
    cp bin/main "$BINARY_DEST"
else
    echo "❌ Could not find whisper binary"
    exit 1
fi

echo "✅ whisper-main built → $BINARY_DEST"
file "$BINARY_DEST"
