# local-dictate

Work in progress — native **Tauri 2** desktop app: microphone capture, silence detection, local speech-to-text, paste into the active application via keyboard shortcut.

Implementation checklist: [`PLAN.md`](./PLAN.md). Build and usage instructions will be completed before submission.

### Whisper model (local STT)

Models are not in git. From the repo root:

```bash
pnpm download-model
```

Default: `models/ggml-base.bin`. Optional: `WHISPER_MODEL=ggml-small.bin pnpm download-model`.
