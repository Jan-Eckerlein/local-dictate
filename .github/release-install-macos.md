## macOS install (unsigned)

These builds are **not notarized**. After installing from the DMG (drag **local-dictate** to Applications), clear Gatekeeper quarantine:

```bash
xattr -cr /Applications/local-dictate.app
```

If macOS still says the app is “damaged”, run the command above, or **right‑click → Open** once. Whisper **ggml-base.bin** is bundled in tagged releases — no extra download for the default model.
