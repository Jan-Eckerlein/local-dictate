# Implementation plan — `local-dictate`

Living checklist for the take-home. Safe to commit (no private briefing text).  
Detailed assignment notes stay local in gitignored files.

**Stack (locked):** Tauri 2 · Rust · React · TypeScript · Vite 8 · macOS first  
**Inspiration (product only):** [Voicely / Everlast community](https://www.voicely.de/en/everlast-community)

---

## Goal

Native desktop mini-app: capture microphone audio until silence, transcribe **locally** (whisper.cpp via Rust bindings), insert text into the **foreground application** via a **global shortcut**. UI is status/feedback only — capture and paste are **Rust / OS APIs**, not browser MediaRecorder or web-only paste.

---

## Architecture (target)

Vertical layout so the diagram stays **narrow** and renders larger in preview / GitHub.

```mermaid
flowchart TB
  UI["React + TypeScript<br/>state · level meter · transcript · errors"]
  BR["Tauri 2<br/>commands · events"]
  UI <--> BR

  subgraph AUDIO["Rust — audio thread"]
    direction TB
    CPAL["cpal — mic"]
    RS["resample 16 kHz mono"]
    VAD["VAD — RMS + silence hang"]
    SEG["segment f32"]
    CPAL --> RS --> VAD --> SEG
  end

  subgraph STT["Rust — STT worker"]
    direction TB
    CH["mpsc"]
    WH["whisper-rs"]
    CH --> WH
  end

  subgraph PASTE["Rust — shortcut"]
    direction TB
    HK["global hotkey"]
    CB["arboard"]
    KV["⌘V"]
    HK --> CB --> KV
  end

  MIC([System mic]) --> CPAL
  BR --> CPAL
  BR --> HK
  SEG --> CH
  WH --> BR
  VAD -.->|RMS| BR
  KV --> APP([Foreground app])
```

**Threading:** `cpal` callback must **never** block on Whisper; segments go to STT via **`mpsc`** on a worker thread.  
**Acceptance:** paste works in **TextEdit / Notes**, not only in the webview.

---

## Decisions

| Topic      | Choice                            | Notes                                                                                |
| ---------- | --------------------------------- | ------------------------------------------------------------------------------------ |
| Platform   | macOS                             | Windows documented as untested unless time allows                                    |
| STT        | `whisper-rs` + ggml on disk       | Model **not** in git; download script; language **auto/de/en** in settings.json      |
| Model size | `base` (default)                  | `pnpm download-model` / env; **UI selector at #14** (tiny/base/small, files on disk) |
| VAD        | RMS + silence hang + max duration | Default **0.008**; user threshold in `app_data_dir/settings.json`                 |
| Paste      | Clipboard + simulated ⌘V          | Accessibility permission likely required                                             |
| UI         | Small window + meter              | Minimal HTML/CSS for milestones 4–9; **shadcn** + Tailwind at **#10**; tray deferred |

**Open (pick during build):**

- [ ] Shortcut scheme: one key vs record / paste split
- [ ] Assignment receipt date in README (deadline line)

---

## Milestones & commits

Each step should compile or be an obvious WIP milestone.

| #   | Milestone                                                           | Status              |
| --- | ------------------------------------------------------------------- | ------------------- |
| 0   | Repo scaffold, `.gitignore`, license (MIT)                          | done (pre-template) |
| 1   | Tauri 2 + React + dev/build working                                 | done                |
| 2   | `cpal` capture + RMS → UI level meter                               | done                |
| 3   | `cpal` input device list + UI selector (avoid wrong default mic)    | done                |
| 4   | Resample 16 kHz mono + silence segmentation                         | done                |
| 5   | cpal replay after stop                                              | done                |
| 6   | `whisper-rs` + model download script                                | done                |
| 7   | Auto STT pipeline: VAD segment → `mpsc` whisper worker → live transcript (no manual Transcribe) | done |
| 8   | User settings (persist): VAD RMS threshold; Whisper language (auto / de / en) | done        |
| 9   | Global shortcut + paste (TextEdit / Notes test)                     | todo                |
| 10  | Tailwind + shadcn foundation (Button, Select, Alert, …)             | todo                |
| 11  | UI state machine + error / permission hints                         | todo                |
| 12  | README (build, model, shortcuts, architecture) + `docs/ai-usage.md` | todo                |
| 13  | Release build (`pnpm tauri build`) + known limitations              | todo                |
| 14  | Whisper **model** setting: persist choice (e.g. tiny/base/small), pick among downloaded `models/*.bin`, reload engine when changed | todo |

### Milestone #14 notes (Whisper model)

- Extend `settings.json` with `whisperModel` (filename or enum → `ggml-*.bin`).
- UI: select next to language; show path + “missing — run download” if file absent.
- On change: reload `WhisperEngine` in worker (or lazy on next segment); do **not** block cpal.
- Keep **default `base`** for fresh clones; README documents `WHISPER_MODEL=… pnpm download-model` for extras.

### Milestone #7 notes (auto STT pipeline)

Not “prettier UI” alone — **#6** proved Whisper works; **#7** wires production flow: segment closes → queue → background transcribe → UI updates while monitoring (manual **Transcribe** stays optional/debug).
- VAD finished segment → **`mpsc`** → dedicated Whisper worker thread (never block cpal); `transcript-ready` (or similar) + app state.
- **VAD pre-roll (observed):** segments cut in on the first loud 20 ms frame — replay sounds correct but **starts abruptly**; Whisper often garbles the **first words** (e.g. “eins zwei” → “ein sei”). Not a loudness issue when replay is intelligible.
  - Add ~**200–300 ms** ring pre-roll at 16 kHz prepended when speech is confirmed (or constant rolling buffer while monitoring).
  - Optional **speech onset**: require **2–3 consecutive** speech frames before `recording` starts (tune with pre-roll so word attacks are not clipped).
  - Re-test with same phrase: “test eins zwei drei …” vs. manual transcribe baseline.
- **Level / SNR (observed):** transcript quality is clearly better when speaking **louder** than quietly (same wording). Quiet speech → lower sample amplitudes, VAD rides closer to threshold, Whisper gets weaker SNR — not only a “cut-in too fast” issue. Consider optional **peak normalize** before STT (cap ~0.9) if quiet dictation must work; document “mic gain / distance” for acceptance demos.
- **Language:** Until **#8**, keep **auto-detect** (`set_language(None)`, `translate(false)`). Settings add persisted **auto / de / en** → `FullParams::set_language`.

---

## macOS permissions (document in README)

- Microphone — when using `cpal`
- Accessibility — likely for synthetic paste

---

## Acceptance checks

- [ ] Speak → silence → local transcript (no cloud)
- [ ] Global shortcut inserts into **another** app (e.g. TextEdit)
- [ ] Explain: sample rate, buffers, VAD constants, ownership of audio buffer, paste path
- [ ] `cargo check` / `pnpm tauri build` clean on a fresh clone (with model download steps)

---

## Tooling notes

- **Vite 8 + pnpm:** explicit `@rolldown/binding-darwin-arm64` pinned to match Vite’s Rolldown version
- **Node:** prefer 22.12+ (Vite 8 warns on 22.11)
- Private briefings: `ASSIGNMENT.md`, `HANDOFF.md` (gitignored)

---

## After submission

- Replace this file’s checklist with “Implemented” summary in README or archive section if desired
- Keep commit history granular for live code review
