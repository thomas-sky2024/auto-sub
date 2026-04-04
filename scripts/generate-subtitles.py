#!/usr/bin/env python3
"""
AutoSub — generate-subtitles.py
Sinh file .srt từ audio/video dùng sherpa-onnx Python API + VAD
Chạy: python3 generate-subtitles.py --model-dir ~/.autosub/models/sense-voice-2024 \
                                     --vad ~/.autosub/models/silero_vad.onnx \
                                     --threads 6 \
                                     /path/to/audio.wav

Output ra stdout dạng JSON lines để Rust pipeline đọc:
{"start": 0.24, "end": 3.52, "text": "你好世界"}
{"start": 4.10, "end": 7.80, "text": "这是第二句话"}

Thoát với code 0 nếu thành công, code 1 nếu lỗi (stderr).
"""

import sys
import json
import argparse
import os

def parse_args():
    p = argparse.ArgumentParser()
    p.add_argument("--model-dir", required=True,
                   help="Thư mục chứa model.int8.onnx và tokens.txt")
    p.add_argument("--model-type", default="sense-voice",
                   choices=["sense-voice", "paraformer", "fire-red-v2"],
                   help="Loại model")
    p.add_argument("--vad", required=True,
                   help="Path đến silero_vad.onnx")
    p.add_argument("--threads", type=int, default=4,
                   help="Số threads CPU")
    p.add_argument("--language", default="auto",
                   help="Ngôn ngữ: auto, zh, en, ja, ko, yue")
    p.add_argument("audio", help="File audio/video WAV cần transcribe")
    return p.parse_args()


def load_sherpa_onnx():
    """Import sherpa_onnx — báo lỗi rõ ràng nếu chưa cài."""
    try:
        import sherpa_onnx
        return sherpa_onnx
    except ImportError:
        print(
            json.dumps({
                "error": "sherpa_onnx chưa được cài. Chạy:\n"
                         "pip3 install sherpa-onnx --break-system-packages\n"
                         "hoặc: pip3 install sherpa-onnx"
            }),
            file=sys.stderr
        )
        sys.exit(1)


def create_recognizer(sherpa_onnx, args):
    model_dir = args.model_dir

    if args.model_type == "sense-voice":
        # Thử int8 trước, fallback sang fp32
        model_path = os.path.join(model_dir, "model.int8.onnx")
        if not os.path.exists(model_path):
            model_path = os.path.join(model_dir, "model.onnx")
        tokens_path = os.path.join(model_dir, "tokens.txt")

        recognizer = sherpa_onnx.OfflineRecognizer.from_sense_voice(
            model=model_path,
            tokens=tokens_path,
            num_threads=args.threads,
            use_itn=True,
            debug=False,
        )

    elif args.model_type == "paraformer":
        model_path = os.path.join(model_dir, "model.int8.onnx")
        tokens_path = os.path.join(model_dir, "tokens.txt")

        recognizer = sherpa_onnx.OfflineRecognizer.from_paraformer(
            paraformer=model_path,
            tokens=tokens_path,
            num_threads=args.threads,
            debug=False,
        )

    elif args.model_type == "fire-red-v2":
        encoder = os.path.join(model_dir, "encoder.int8.onnx")
        decoder = os.path.join(model_dir, "decoder.int8.onnx")
        tokens_path = os.path.join(model_dir, "tokens.txt")

        recognizer = sherpa_onnx.OfflineRecognizer.from_fire_red_asr(
            encoder=encoder,
            decoder=decoder,
            tokens=tokens_path,
            num_threads=args.threads,
            debug=False,
        )
    else:
        raise ValueError(f"Unknown model type: {args.model_type}")

    return recognizer


def create_vad(sherpa_onnx, vad_path, sample_rate=16000):
    config = sherpa_onnx.VadModelConfig()
    config.silero_vad.model = vad_path
    config.silero_vad.threshold = 0.5
    config.silero_vad.min_silence_duration = 0.3
    config.silero_vad.max_speech_duration = 29.0
    config.sample_rate = sample_rate
    config.num_threads = 1

    vad = sherpa_onnx.VoiceActivityDetector(config, buffer_size_in_seconds=100)
    return vad


def read_wave(wave_filename):
    """Đọc WAV file, trả về (samples_float32, sample_rate)."""
    import wave
    import struct

    with wave.open(wave_filename) as f:
        num_channels = f.getnchannels()
        sample_width = f.getsampwidth()
        sample_rate = f.getframerate()
        num_frames = f.getnframes()

        frames = f.readframes(num_frames)

    if sample_width == 2:
        samples = struct.unpack(f"<{num_frames * num_channels}h", frames)
        samples = [s / 32768.0 for s in samples]
        # Mix to mono nếu stereo
        if num_channels > 1:
            samples = [
                sum(samples[i:i+num_channels]) / num_channels
                for i in range(0, len(samples), num_channels)
            ]
    else:
        raise ValueError(f"Unsupported sample width: {sample_width}")

    return samples, sample_rate


def transcribe(args):
    sherpa_onnx = load_sherpa_onnx()

    # Validate inputs
    if not os.path.exists(args.audio):
        print(json.dumps({"error": f"Audio file not found: {args.audio}"}), file=sys.stderr)
        sys.exit(1)

    if not os.path.exists(args.vad):
        print(json.dumps({"error": f"VAD model not found: {args.vad}"}), file=sys.stderr)
        sys.exit(1)

    model_onnx = os.path.join(args.model_dir, "model.int8.onnx")
    if not os.path.exists(model_onnx):
        model_onnx = os.path.join(args.model_dir, "model.onnx")
    if not os.path.exists(model_onnx) and args.model_type == "sense-voice":
        print(json.dumps({"error": f"Model not found in: {args.model_dir}"}), file=sys.stderr)
        sys.exit(1)

    # Đọc audio
    try:
        samples, sample_rate = read_wave(args.audio)
    except Exception as e:
        print(json.dumps({"error": f"Cannot read audio: {e}"}), file=sys.stderr)
        sys.exit(1)

    if sample_rate != 16000:
        print(json.dumps({"error": f"Audio sample rate must be 16000, got {sample_rate}"}), file=sys.stderr)
        sys.exit(1)

    # Tạo recognizer + VAD
    try:
        recognizer = create_recognizer(sherpa_onnx, args)
        vad = create_vad(sherpa_onnx, args.vad, sample_rate)
    except Exception as e:
        print(json.dumps({"error": f"Model load failed: {e}"}), file=sys.stderr)
        sys.exit(1)

    # Xử lý VAD + ASR
    import numpy as np

    samples_np = np.array(samples, dtype=np.float32)
    vad.accept_waveform(samples_np)
    vad.flush()

    segments = []

    while not vad.empty():
        speech_segment = vad.front()
        vad.pop()

        start_time = speech_segment.start / sample_rate
        end_time = (speech_segment.start + len(speech_segment.samples)) / sample_rate

        stream = recognizer.create_stream()
        stream.accept_waveform(sample_rate, speech_segment.samples)
        recognizer.decode_stream(stream)

        text = stream.result.text.strip()

        if text:
            # Xuất ra stdout dạng JSON line để Rust đọc
            result = {
                "start": round(start_time, 3),
                "end": round(end_time, 3),
                "text": text
            }
            print(json.dumps(result, ensure_ascii=False), flush=True)
            segments.append(result)

    return len(segments)


def main():
    args = parse_args()

    try:
        count = transcribe(args)
        # Kết thúc với summary trên stderr (Rust bỏ qua)
        print(f"Done: {count} segments", file=sys.stderr)
        sys.exit(0)
    except KeyboardInterrupt:
        sys.exit(1)
    except Exception as e:
        print(json.dumps({"error": str(e)}), file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
