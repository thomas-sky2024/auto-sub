#!/bin/bash
# build-scripts/build-sensevoice.sh
# 
# 1. Tải Python Portable (python-build-standalone) cho macOS
# 2. Cài đặt các thư viện cần thiết vào môi trường này
# 3. Tạo sidecar wrapper trỏ vào Python nội bộ này
#
# Giúp ứng dụng chạy không cần cài đặt Python trên máy người dùng.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SCRIPTS_DIR="$REPO_DIR/scripts"
BIN_DEST="$REPO_DIR/src-tauri/binaries"
# Store python in src-tauri/resources to be bundled by Tauri
PYTHON_BUNDLE_DIR="$REPO_DIR/src-tauri/resources/python"

mkdir -p "$BIN_DEST"
mkdir -p "$SCRIPTS_DIR"
mkdir -p "$REPO_DIR/src-tauri/resources"

ARCH=$(uname -m)
if [ "$ARCH" = "arm64" ]; then
    TARGET_TRIPLE="aarch64-apple-darwin"
    PYTHON_URL="https://github.com/astral-sh/python-build-standalone/releases/download/20260325/cpython-3.11.15+20260325-aarch64-apple-darwin-install_only.tar.gz"
else
    # Mặc định là x86_64 cho Intel Mac
    TARGET_TRIPLE="x86_64-apple-darwin"
    PYTHON_URL="https://github.com/astral-sh/python-build-standalone/releases/download/20260325/x86_64-apple-darwin-install_only.tar.gz"
fi

echo "🔍 Arch: $ARCH → Target: $TARGET_TRIPLE"
echo "📂 Python Bundle: $PYTHON_BUNDLE_DIR"
echo ""

# ── Bước 1: Tải & Giải nén Python Portable ──────────────────────────────────
if [ ! -d "$PYTHON_BUNDLE_DIR" ] || [ ! -f "$PYTHON_BUNDLE_DIR/bin/python3" ]; then
    echo "📥 Đang tải Python Portable (minimal)..."
    TEMP_DIR=$(mktemp -d)
    curl -L "$PYTHON_URL" -o "$TEMP_DIR/python.tar.gz"
    
    echo "📦 Đang giải nén..."
    mkdir -p "$PYTHON_BUNDLE_DIR"
    tar -xzf "$TEMP_DIR/python.tar.gz" -C "$PYTHON_BUNDLE_DIR" --strip-components=1
    rm -rf "$TEMP_DIR"
    echo "✅ Đã setup Python nội bộ tại: $PYTHON_BUNDLE_DIR"
else
    echo "✅ Đã có sẵn Python nội bộ tại: $PYTHON_BUNDLE_DIR"
fi

BUNDLE_PYTHON="$PYTHON_BUNDLE_DIR/bin/python3"

# ── Bước 2: Cài đặt dependencies vào Python nội bộ ──────────────────────────
echo "📦 Cài đặt thư viện (sherpa-onnx, numpy) vào Python bundle..."

# Cập nhật pip trước
"$BUNDLE_PYTHON" -m pip install --upgrade pip --quiet

# Cài đặt các package cần thiết
"$BUNDLE_PYTHON" -m pip install sherpa-onnx numpy --quiet

# Verify
VERSION=$("$BUNDLE_PYTHON" -c "import sherpa_onnx; print(sherpa_onnx.__version__)")
echo "✅ Đã cài sherpa-onnx v$VERSION vào bundle."
echo ""

# ── Bước 3: Tạo wrapper shell script làm "binary" cho Tauri ─────────────────
WRAPPER="$BIN_DEST/sherpa-onnx-vad-${TARGET_TRIPLE}"

cat > "$WRAPPER" << 'WRAPPER_EOF'
#!/bin/bash
# AutoSub sherpa-onnx wrapper (Bundled Python Version)
# Wrapper này sẽ tìm và sử dụng Python được đóng gói kèm theo app.

BINARY_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 1. Tìm Python nội bộ
# - Trong Dev: binaries/../../src-tauri/resources/python/bin/python3 (No, depends on structure)
# - Trong Prod: Contents/MacOS/../../Resources/python/bin/python3
# Thử nhiều vị trí để đảm bảo chạy được cả ở chế độ dev và prod.

PYTHON=""
for candidate in \
    "$BINARY_DIR/../../Resources/python/bin/python3" \
    "$BINARY_DIR/../resources/python/bin/python3" \
    "$BINARY_DIR/../../resources/python/bin/python3" \
    "$(cd "$BINARY_DIR/../../.." && pwd)/src-tauri/resources/python/bin/python3"; do
    if [ -x "$candidate" ]; then
        PYTHON="$candidate"
        break
    fi
done

# Fallback cuối cùng nếu không tìm thấy bundle (có thể là khi dev chưa chạy build script)
if [ -z "$PYTHON" ]; then
    PYTHON=$(which python3)
fi

# 2. Lấy đường dẫn script từ đối số đầu tiên (do Rust truyền vào)
if [ -z "$1" ]; then
    echo '{"error": "Không nhận được đường dẫn Python script từ Rust"}' >&2
    exit 1
fi

SCRIPT="$1"
shift

if [ ! -f "$SCRIPT" ]; then
    echo "{\"error\": \"Không tìm thấy file script tại: $SCRIPT\"}" >&2
    exit 1
fi

# 3. Thực thi
exec "$PYTHON" "$SCRIPT" "$@"
WRAPPER_EOF

chmod +x "$WRAPPER"
echo "✅ Wrapper binary (VAD): $WRAPPER"

# Tạo thêm offline wrapper (dùng chung logic) để Tauri không báo lỗi
OFFLINE_WRAPPER="$BIN_DEST/sherpa-onnx-offline-${TARGET_TRIPLE}"
cp "$WRAPPER" "$OFFLINE_WRAPPER"
echo "✅ Wrapper binary (Offline): $OFFLINE_WRAPPER"

echo ""
echo "🧪 Test wrapper với Python bundle..."
# Truyền script path thực tế vào để test wrapper (vì wrapper yêu cầu $1 là script path)
if "$WRAPPER" "$SCRIPTS_DIR/generate-subtitles.py" --help >/dev/null 2>&1; then
    echo "✅ Wrapper test OK (Sử dụng Python bundle thành công)"
else
    echo "⚠️  Wrapper test failed — kiểm tra lại cấu hình."
    echo "   Gợi ý: Thử chạy './src-tauri/binaries/sherpa-onnx-vad-${TARGET_TRIPLE} $SCRIPTS_DIR/generate-subtitles.py --help' để xem lỗi chi tiết."
fi

echo ""
echo "══════════════════════════════════════════"
echo "✅ Đã sẵn sàng để đóng gói Zero-Dependency!"
echo "══════════════════════════════════════════"
