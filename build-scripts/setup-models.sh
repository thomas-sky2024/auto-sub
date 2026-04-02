#!/bin/bash
set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WHISPER_MODELS_DIR="$SCRIPT_DIR/whisper.cpp/models"
DEST_DIR="$HOME/.autosub/models"

mkdir -p "$DEST_DIR"

echo "📂 Created models directory at $DEST_DIR"

# Download base, small, medium, large-v2, and large-v3 models if they don't exist
models=("base" "small" "medium" "large-v2" "large-v3")

for model in "${models[@]}"; do
    if [ ! -f "$DEST_DIR/ggml-$model.bin" ]; then
        echo "⬇️  Downloading $model model..."
        bash "$WHISPER_MODELS_DIR/download-ggml-model.sh" "$model" "$DEST_DIR"
    else
        echo "✅ Model $model already exists at $DEST_DIR"
    fi
done

echo "🚀 Model setup complete!"
