#!/bin/bash
set -e

# ── Configuration ─────────────────────────────────────────────────────────────
BASE_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models"
DEST_DIR="$HOME/.autosub/models"
TMPDIR_BASE=$(mktemp -d)
trap 'rm -rf "$TMPDIR_BASE"' EXIT

mkdir -p "$DEST_DIR"
echo "📂 Models directory: $DEST_DIR"
echo ""

# ── Helper Functions ──────────────────────────────────────────────────────────
download_model() {
    local tarball="$1"
    local dest_name="$2"       # tên thư mục đích trong $DEST_DIR
    local check_file="$3"      # file cần kiểm tra để skip nếu đã có
    local min_size_mb="${4:-5}" # size tối thiểu để coi là valid (MB)

    local dest_path="$DEST_DIR/$dest_name"
    local check_path="$dest_path/$check_file"

    if [ -f "$check_path" ]; then
        local size_bytes
        size_bytes=$(wc -c < "$check_path" 2>/dev/null || echo 0)
        local min_bytes=$((min_size_mb * 1024 * 1024))
        if [ "$size_bytes" -gt "$min_bytes" ]; then
            echo "✅ $dest_name — đã có sẵn, bỏ qua"
            return 0
        fi
    fi

    echo "⬇️  Đang tải $dest_name ($tarball)..."
    local tmp_file="$TMPDIR_BASE/$tarball"

    if ! curl -L --progress-bar --retry 3 \
        "$BASE_URL/$tarball" \
        -o "$tmp_file"; then
        echo "❌ Tải thất bại: $tarball"
        return 1
    fi

    echo "📦 Đang giải nén..."
    mkdir -p "$dest_path"
    case "$tarball" in
        *.tar.bz2) tar -xjf "$tmp_file" -C "$TMPDIR_BASE" ;;
        *.tar.gz)  tar -xzf "$tmp_file" -C "$TMPDIR_BASE" ;;
        *.zip)     unzip -q "$tmp_file" -d "$TMPDIR_BASE" ;;
        *.onnx)    cp "$tmp_file" "$dest_path/" && return 0 ;;
        *)         echo "❓ Unrecognized format: $tarball" && return 1 ;;
    esac

    # Tìm thư mục được giải nén (khác với tmp_file)
    local extracted_dir
    extracted_dir=$(find "$TMPDIR_BASE" -maxdepth 1 -mindepth 1 -type d | head -1)

    if [ -z "$extracted_dir" ]; then
        echo "❌ Không tìm thấy thư mục sau khi giải nén"
        return 1
    fi

    # Copy tất cả files vào dest (không lấy test_wavs)
    rsync -a --exclude="test_wavs" --exclude="*.sh" \
        "$extracted_dir/" "$dest_path/"

    # Cleanup extracted
    rm -rf "$extracted_dir"
    rm -f "$tmp_file"

    echo "✅ $dest_name — sẵn sàng"
}

# ── Parse Arguments ───────────────────────────────────────────────────────────
MODELS_TO_INSTALL=()

if [ $# -eq 0 ]; then
    echo "📋 Sử dụng: $0 [--all | --vad | --sense-voice-2024 | --sense-voice-2025 | --paraformer-zh | --fire-red-v2]"
    echo ""
    echo "Danh sách models:"
    echo "  --vad              Silero VAD (~2MB, BẮT BUỘC)"
    echo "  --sense-voice-2024 SenseVoice-Small int8 2024 (~60MB, zh/en/ja/ko/yue)"
    echo "  --sense-voice-2025 SenseVoice-Small int8 2025 (~60MB, cải tiến hơn 2024)"
    echo "  --paraformer-zh    Paraformer-zh int8 2025 (~220MB, tiếng Trung chuyên sâu)"
    echo "  --fire-red-v2      FireRedASR v2 CTC int8 (~250MB, zh/en + 20 phương ngữ)"
    echo "  --all              Tải tất cả models"
    echo ""
    echo "Ví dụ: $0 --vad --sense-voice-2024"
    exit 0
fi

for arg in "$@"; do
    case "$arg" in
        --all)
            MODELS_TO_INSTALL=("vad" "sense-voice-2024" "sense-voice-2025" "paraformer-zh" "fire-red-v2")
            ;;
        --vad)                MODELS_TO_INSTALL+=("vad") ;;
        --sense-voice-2024)   MODELS_TO_INSTALL+=("sense-voice-2024") ;;
        --sense-voice-2025)   MODELS_TO_INSTALL+=("sense-voice-2025") ;;
        --paraformer-zh)      MODELS_TO_INSTALL+=("paraformer-zh") ;;
        --fire-red-v2)        MODELS_TO_INSTALL+=("fire-red-v2") ;;
        *)
            echo "⚠️  Tham số không hợp lệ: $arg"
            exit 1
            ;;
    esac
done

# Remove duplicates
MODELS_TO_INSTALL=($(echo "${MODELS_TO_INSTALL[@]}" | tr ' ' '\n' | sort -u | tr '\n' ' '))

echo "🎯 Models sẽ cài: ${MODELS_TO_INSTALL[*]}"
echo "──────────────────────────────────────────────────"

# ── Install Each Model ────────────────────────────────────────────────────────
for model in "${MODELS_TO_INSTALL[@]}"; do
    case "$model" in
        "vad")
            # Silero VAD — single file, không cần tarball
            VAD_PATH="$DEST_DIR/silero_vad.onnx"
            if [ -f "$VAD_PATH" ] && [ "$(wc -c < "$VAD_PATH")" -gt 1000000 ]; then
                echo "✅ Silero VAD — đã có sẵn"
            else
                echo "⬇️  Đang tải Silero VAD (~2MB)..."
                curl -L --progress-bar --retry 3 \
                    "$BASE_URL/silero_vad.onnx" \
                    -o "$VAD_PATH"
                echo "✅ Silero VAD — sẵn sàng"
            fi
            ;;

        "sense-voice-2024")
            download_model \
                "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2" \
                "sense-voice-2024" \
                "model.int8.onnx" \
                40
            ;;

        "sense-voice-2025")
            # Thử tarball 2025-09-09 trước
            TARBALL_2025="sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2"
            download_model \
                "$TARBALL_2025" \
                "sense-voice-2025" \
                "model.int8.onnx" \
                40
            # Nếu thất bại, file model có thể tên khác (không có .int8)
            if [ ! -f "$DEST_DIR/sense-voice-2025/model.int8.onnx" ] && \
               [ -f "$DEST_DIR/sense-voice-2025/model.onnx" ]; then
                cp "$DEST_DIR/sense-voice-2025/model.onnx" \
                   "$DEST_DIR/sense-voice-2025/model.int8.onnx"
            fi
            ;;

        "paraformer-zh")
            download_model \
                "sherpa-onnx-paraformer-zh-int8-2025-10-07.tar.bz2" \
                "paraformer-zh" \
                "model.int8.onnx" \
                100
            ;;

        "fire-red-v2")
            download_model \
                "sherpa-onnx-fire-red-asr2-ctc-zh_en-int8-2026-02-25.tar.bz2" \
                "fire-red-v2" \
                "encoder.int8.onnx" \
                100
            ;;
    esac
done

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════════════════"
echo "📊 Tóm tắt:"
echo ""

check_and_print() {
    local label="$1"
    local file="$2"
    local min_mb="${3:-1}"
    if [ -f "$file" ] && [ "$(wc -c < "$file")" -gt $((min_mb * 1024 * 1024)) ]; then
        local size_mb=$(( $(wc -c < "$file") / 1024 / 1024 ))
        echo "  ✅ $label (${size_mb}MB)"
    else
        echo "  ❌ $label — chưa có"
    fi
}

check_and_print "Silero VAD" "$DEST_DIR/silero_vad.onnx" 1
check_and_print "SenseVoice 2024" "$DEST_DIR/sense-voice-2024/model.int8.onnx" 30
check_and_print "SenseVoice 2025" "$DEST_DIR/sense-voice-2025/model.int8.onnx" 30
check_and_print "Paraformer-zh" "$DEST_DIR/paraformer-zh/model.int8.onnx" 100
check_and_print "FireRedASR v2" "$DEST_DIR/fire-red-v2/encoder.int8.onnx" 50

echo ""
echo "📁 Vị trí: $DEST_DIR"
echo "══════════════════════════════════════════════════"
echo ""
echo "⚠️  Lưu ý: Silero VAD là BẮT BUỘC cho tất cả models."
echo "   Chạy: $0 --vad trước nếu chưa có."
