#!/usr/bin/env bash
# Downloads tiny, base, and small whisper.cpp models into ./models (gitignored).
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
for name in ggml-tiny.bin ggml-base.bin ggml-small.bin; do
  WHISPER_MODEL="$name" bash "$DIR/download-whisper-model.sh"
done
