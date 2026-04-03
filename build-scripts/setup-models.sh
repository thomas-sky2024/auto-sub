#!/bin/bash
set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WHISPER_MODELS_DIR="$SCRIPT_DIR/whisper.cpp/models"
DEST_DIR="$HOME/.autosub/models"

mkdir -p "$DEST_DIR"

echo "📂 Created models directory at $DEST_DIR"

# Download medium, large-v2, and large-v3-turbo models if they don't exist
models=("medium" "large-v2" "large-v3-turbo")

for model in "${models[@]}"; do
    if [ ! -f "$DEST_DIR/ggml-$model.bin" ]; then
        echo "⬇️  Downloading $model model..."
        bash "$WHISPER_MODELS_DIR/download-ggml-model.sh" "$model" "$DEST_DIR"
    else
        echo "✅ Model $model already exists at $DEST_DIR"
    fi
done

# Download Silero VAD model for voice activity detection
echo ""
echo "⬇️  Setting up Silero VAD model for real-time filtering..."
VAD_MODEL_PATH="$DEST_DIR/silero_vad2.onnx"
if [ ! -f "$VAD_MODEL_PATH" ]; then
    echo "Downloading Silero VAD2 model..."
    curl -L -o "$VAD_MODEL_PATH" \
        "https://github.com/snakers4/silero-vad/raw/master/files/silero_vad.onnx" \
        --progress-bar || {
        echo "❌ Failed to download VAD model"
        exit 1
    }
    echo "✅ Silero VAD model downloaded"
else
    echo "✅ Silero VAD model already exists at $VAD_MODEL_PATH"
fi

echo "🚀 Model setup complete!"
