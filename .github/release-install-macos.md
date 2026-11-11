## macOS install (unsigned)

These builds are **not notarized**. After installing from the DMG (drag **local-dictate** to Applications), clear Gatekeeper quarantine:

```bash
xattr -cr /Applications/local-dictate.app
```

If macOS still says the app is “damaged”, run the command above, or **right‑click → Open** once.

Tagged releases bundle **tiny**, **base**, and **small** Whisper models (~680 MB total). Default in the app is **base**; switch under Settings → Whisper model.
