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

| Topic      | Choice                            | Notes                                                                                 |
| ---------- | --------------------------------- | ------------------------------------------------------------------------------------- |
| Platform   | macOS                             | Windows documented as untested unless time allows                                     |
| STT        | `whisper-rs` + ggml on disk       | Model **not** in git; download script; language **auto/de/en** in settings.json       |
| Model size | `base` (default)                  | `pnpm download-model` / env; **UI selector at #16** (tiny/base/small, files on disk)  |
| VAD        | RMS + silence hang + max duration | Default **0.008**; user threshold in `app_data_dir/settings.json`                     |
| Paste      | Clipboard + simulated ⌘V          | **⌘⇧V** / **⌘⇧B** / **⌘⇧A**; Accessibility required (macOS)                           |
| UI         | Small window + meter              | Minimal HTML/CSS for milestones 4–11; **shadcn** + Tailwind at **#12**; tray deferred |

**Open (pick during build):**

- [x] Paste shortcuts: **⌘⇧V** last segment · **⌘⇧B** since last batch · **⌘⇧A** all (monitoring via UI)
- [ ] Assignment receipt date in README (deadline line)

---

## Milestones & commits

Each step should compile or be an obvious WIP milestone.

| #   | Milestone                                                                                                                          | Status              |
| --- | ---------------------------------------------------------------------------------------------------------------------------------- | ------------------- |
| 0   | Repo scaffold, `.gitignore`, license (MIT)                                                                                         | done (pre-template) |
| 1   | Tauri 2 + React + dev/build working                                                                                                | done                |
| 2   | `cpal` capture + RMS → UI level meter                                                                                              | done                |
| 3   | `cpal` input device list + UI selector (avoid wrong default mic)                                                                   | done                |
| 4   | Resample 16 kHz mono + silence segmentation                                                                                        | done                |
| 5   | cpal replay after stop                                                                                                             | done                |
| 6   | `whisper-rs` + model download script                                                                                               | done                |
| 7   | Auto STT pipeline: VAD segment → `mpsc` whisper worker → live transcript (no manual Transcribe)                                    | done                |
| 8   | User settings (persist): VAD RMS threshold; Whisper language (auto / de / en)                                                      | done                |
| 9   | Global shortcut + paste (TextEdit / Notes test)                                                                                    | done                |
| 10  | VAD **pre-roll** (~300 ms) + **speech onset** (3×20 ms frames) before record                                                       | done                |
| 11  | **Peak normalize** quiet segments before Whisper (`normalize_peak_for_stt`)                                                        | done                |
| 12  | Tailwind + shadcn foundation (Button, Select, Alert, …)                                                                            | done                |
| 13  | UI state machine + error / permission hints                                                                                        | done                |
| 14  | README (build, model, shortcuts, architecture) + `docs/ai-usage.md`                                                                | done                |
| 15  | Release build (`pnpm tauri build`) + known limitations                                                                             | done                |
| 16  | Whisper **model** setting: persist choice (e.g. tiny/base/small), pick among downloaded `models/*.bin`, reload engine when changed | todo                |

### Milestone #10 notes (VAD pre-roll)

- Constants in `segment.rs`: `VAD_PREROLL_MS`, `VAD_SPEECH_ONSET_FRAMES`.
- Ring buffer while monitoring; prepend on confirmed onset (fewer clipped first words).
- Re-test: “test eins zwei drei …” vs. replay / manual transcribe.

### Milestone #15 notes (release + CI)

- `src-tauri/.cargo/config.toml`: `MACOSX_DEPLOYMENT_TARGET=11.0` (whisper.cpp compile).
- `tauri.conf.json` `minimumSystemVersion`: `11.0`.
- `.github/workflows/ci.yml`: Ubuntu frontend build + macOS `pnpm tauri build`, bundle artifacts.
- CI bundles are **unsigned**; Whisper model still via `pnpm download-model` locally.

### Milestone #14 notes (README)

- `README.md`: requirements, quick start, model download, shortcuts, permissions, architecture diagram, VAD constants, settings path.
- `docs/ai-usage.md`: tools, verification scope, commit audit trail for review.

### Milestone #13 notes (UI phase)

- `deriveUiPhase` + `buildAppStatus` in `src/lib/ui-phase.ts`; compact phase label in header.
- shadcn **Sonner** toasts (bottom): info blue · alert amber · error red — no inline status alerts.
- Start disabled until mic + model OK; paste gated on AX + transcript.

### Milestone #11 notes (peak normalize)

- `normalize_peak_for_stt` in `segment.rs`; applied in `WhisperEngine::transcribe` only (replay/session buffers unchanged).
- Quiet segments (peak &lt; `0.5`, not near-silent) scaled to peak `0.9`; loud segments untouched.

### Milestone #16 notes (Whisper model)

- Extend `settings.json` with `whisperModel` (filename or enum → `ggml-*.bin`).
- UI: select next to language; show path + “missing — run download” if file absent.
- On change: reload `WhisperEngine` in worker (or lazy on next segment); do **not** block cpal.
- Keep **default `base`** for fresh clones; README documents `WHISPER_MODEL=… pnpm download-model` for extras.

### Milestone #7 notes (auto STT pipeline)

Not “prettier UI” alone — **#6** proved Whisper works; **#7** wires production flow: segment closes → queue → background transcribe → UI updates while monitoring (manual **Transcribe** stays optional/debug).

- VAD finished segment → **`mpsc`** → dedicated Whisper worker thread (never block cpal); `transcript-ready` (or similar) + app state.
- **VAD pre-roll:** see **#10** (implemented).
- **Level / SNR:** peak normalize before Whisper — see **#11**. Still document mic gain / distance for quiet dictation demos.
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
- [x] `cargo check` / `pnpm tauri build` clean on a fresh clone (with model download steps) — CI macOS job + local verify

---

## Tooling notes

- **Vite 8 + pnpm:** explicit `@rolldown/binding-darwin-arm64` pinned to match Vite’s Rolldown version
- **Node:** prefer 22.12+ (Vite 8 warns on 22.11)
- Private briefings: `ASSIGNMENT.md`, `HANDOFF.md` (gitignored)

---

## After submission

- Replace this file’s checklist with “Implemented” summary in README or archive section if desired
- Keep commit history granular for live code review
