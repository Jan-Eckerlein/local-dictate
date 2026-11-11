#!/usr/bin/env bash
# Downloads the default whisper.cpp ggml-base model into ./models (gitignored).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODEL_DIR="${MODEL_DIR:-$ROOT/models}"
MODEL_NAME="${WHISPER_MODEL:-ggml-base.bin}"
URL="${WHISPER_MODEL_URL:-https://huggingface.co/ggerganov/whisper.cpp/resolve/main/${MODEL_NAME}}"

mkdir -p "$MODEL_DIR"
OUT="$MODEL_DIR/$MODEL_NAME"

if [[ -f "$OUT" ]]; then
  echo "Model already present: $OUT"
  exit 0
fi

echo "Downloading $URL"
echo "         -> $OUT"
curl -# -L -o "$OUT" "$URL"
echo "Done ($(wc -c <"$OUT" | tr -d ' ') bytes)"
