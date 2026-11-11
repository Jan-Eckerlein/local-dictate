import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import "./App.css";

interface AudioMonitorInfo {
  sampleRate: number;
  channels: number;
  sampleFormat: string;
  deviceName: string;
}

interface AudioLevelPayload {
  level: number;
  rms: number;
}

interface InputDeviceInfo {
  name: string;
  isDefault: boolean;
}

interface AudioSegmentPayload {
  index: number;
  sampleRate: number;
  sampleCount: number;
  durationSecs: number;
}

interface SessionSegmentSummary {
  index: number;
  sampleRate: number;
  sampleCount: number;
  durationSecs: number;
}

interface WhisperModelInfo {
  path: string;
  exists: boolean;
  loaded: boolean;
}

interface TranscriptResult {
  segmentIndex: number;
  text: string;
}

interface TranscriptReadyPayload {
  segmentIndex: number;
  text: string;
}

interface TranscriptErrorPayload {
  segmentIndex: number;
  message: string;
}

type WhisperLanguage = "auto" | "de" | "en";

interface AppSettings {
  vadRmsThreshold: number;
  whisperLanguage: WhisperLanguage;
}

function App() {
  const [monitoring, setMonitoring] = useState(false);
  const [level, setLevel] = useState(0);
  const [info, setInfo] = useState<AudioMonitorInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [micStatus, setMicStatus] = useState("");
  const [devices, setDevices] = useState<InputDeviceInfo[]>([]);
  const [selectedDevice, setSelectedDevice] = useState("");
  const [lastSegment, setLastSegment] = useState<AudioSegmentPayload | null>(null);
  const [segmentCount, setSegmentCount] = useState(0);
  const [sessionSegments, setSessionSegments] = useState<SessionSegmentSummary[]>([]);
  const [replayingIndex, setReplayingIndex] = useState<number | null>(null);
  const [transcribingIndex, setTranscribingIndex] = useState<number | null>(null);
  const [transcripts, setTranscripts] = useState<Record<number, string>>({});
  const [whisperModel, setWhisperModel] = useState<WhisperModelInfo | null>(null);
  const [sttPendingIndex, setSttPendingIndex] = useState<number | null>(null);
  const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
  const [axStatus, setAxStatus] = useState("");
  const [latestTranscript, setLatestTranscript] = useState<string | null>(null);
  const [pasteBusy, setPasteBusy] = useState(false);

  const refreshMicStatus = useCallback(() => {
    void invoke<string>("mic_permission_status").then(setMicStatus);
  }, []);

  const refreshAxStatus = useCallback(() => {
    void invoke<string>("accessibility_permission_status").then(setAxStatus);
  }, []);

  const refreshLatestTranscript = useCallback(() => {
    void invoke<string | null>("get_latest_transcript").then(setLatestTranscript);
  }, []);

  useEffect(() => {
    refreshMicStatus();
    refreshAxStatus();
    refreshLatestTranscript();
    void invoke<WhisperModelInfo>("whisper_model_info").then(setWhisperModel);
    void invoke<AppSettings>("get_app_settings").then(setAppSettings);
  }, [refreshMicStatus, refreshAxStatus, refreshLatestTranscript]);

  const updateVadThreshold = useCallback(async (vadRmsThreshold: number) => {
    try {
      const next = await invoke<AppSettings>("update_app_settings", { vadRmsThreshold });
      setAppSettings(next);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const updateWhisperLanguage = useCallback(async (whisperLanguage: WhisperLanguage) => {
    try {
      const next = await invoke<AppSettings>("update_app_settings", { whisperLanguage });
      setAppSettings(next);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const loadDevices = useCallback(async () => {
    try {
      const list = await invoke<InputDeviceInfo[]>("list_input_audio_devices");
      setDevices(list);
      setSelectedDevice((prev) => {
        if (prev && list.some((d) => d.name === prev)) return prev;
        const fallback = list.find((d) => d.isDefault) ?? list[0];
        return fallback?.name ?? "";
      });
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    if (micStatus === "authorized") {
      void loadDevices();
    }
  }, [micStatus, loadDevices]);

  useEffect(() => {
    let cancelled = false;
    let unlistenLevel: UnlistenFn | undefined;
    let unlistenSegment: UnlistenFn | undefined;
    let unlistenTranscript: UnlistenFn | undefined;
    let unlistenTranscriptErr: UnlistenFn | undefined;
    let unlistenPasteErr: UnlistenFn | undefined;
    let unlistenPasteDone: UnlistenFn | undefined;

    void (async () => {
      unlistenLevel = await listen<AudioLevelPayload>("audio-level", (event) => {
        setLevel(event.payload.level);
      });
      if (cancelled) {
        await unlistenLevel();
        return;
      }

      unlistenSegment = await listen<AudioSegmentPayload>("audio-segment", (event) => {
        setLastSegment(event.payload);
        setSegmentCount(event.payload.index + 1);
        setSttPendingIndex(event.payload.index);
      });
      if (cancelled) {
        await unlistenSegment();
        return;
      }

      unlistenTranscript = await listen<TranscriptReadyPayload>("transcript-ready", (event) => {
        const { segmentIndex, text } = event.payload;
        setTranscripts((prev) => ({ ...prev, [segmentIndex]: text }));
        setLatestTranscript(text.trim() || null);
        setSttPendingIndex((pending) => (pending === segmentIndex ? null : pending));
        void invoke<WhisperModelInfo>("whisper_model_info").then(setWhisperModel);
      });
      if (cancelled) {
        await unlistenTranscript();
        return;
      }

      unlistenTranscriptErr = await listen<TranscriptErrorPayload>("transcript-error", (event) => {
        setError(`STT #${event.payload.segmentIndex + 1}: ${event.payload.message}`);
        setSttPendingIndex((pending) =>
          pending === event.payload.segmentIndex ? null : pending,
        );
      });
      if (cancelled) {
        await unlistenTranscriptErr();
        return;
      }

      unlistenPasteErr = await listen<string>("paste-error", (event) => {
        setError(event.payload);
      });
      if (cancelled) {
        await unlistenPasteErr();
        return;
      }

      unlistenPasteDone = await listen("paste-done", () => {
        setError(null);
      });
      if (cancelled) {
        await unlistenPasteDone();
      }
    })();

    return () => {
      cancelled = true;
      void unlistenLevel?.();
      void unlistenSegment?.();
      void unlistenTranscript?.();
      void unlistenTranscriptErr?.();
      void unlistenPasteErr?.();
      void unlistenPasteDone?.();
    };
  }, []);

  const micAuthorized = micStatus === "authorized";
  const axAuthorized = axStatus === "authorized";

  const openSettings = useCallback(() => {
    void invoke("open_microphone_settings").then(refreshMicStatus);
  }, [refreshMicStatus]);

  const openAxSettings = useCallback(() => {
    void invoke("open_accessibility_settings").then(refreshAxStatus);
  }, [refreshAxStatus]);

  const pasteLast = useCallback(async () => {
    setError(null);
    setPasteBusy(true);
    try {
      await invoke("paste_last_transcript");
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setPasteBusy(false);
      refreshAxStatus();
    }
  }, [refreshAxStatus]);

  const start = useCallback(async () => {
    setError(null);
    try {
      const deviceInfo = await invoke<AudioMonitorInfo>("start_audio_monitor", {
        deviceName: selectedDevice || null,
      });
      setInfo(deviceInfo);
      setLastSegment(null);
      setSegmentCount(0);
      setSessionSegments([]);
      setTranscripts({});
      setSttPendingIndex(null);
      setMonitoring(true);
      refreshMicStatus();
    } catch (e) {
      setMonitoring(false);
      setError(String(e));
      refreshMicStatus();
    }
  }, [refreshMicStatus, selectedDevice]);

  const stop = useCallback(async () => {
    setError(null);
    try {
      await invoke("stop_audio_monitor");
      setMonitoring(false);
      setLevel(0);
      const list = await invoke<SessionSegmentSummary[]>("list_session_segments");
      setSessionSegments(list);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const transcribeSegment = useCallback(async (index: number) => {
    setError(null);
    setTranscribingIndex(index);
    try {
      const result = await invoke<TranscriptResult>("transcribe_session_segment", { index });
      setTranscripts((prev) => ({ ...prev, [result.segmentIndex]: result.text }));
      void invoke<WhisperModelInfo>("whisper_model_info").then(setWhisperModel);
    } catch (e) {
      setError(String(e));
    } finally {
      setTranscribingIndex(null);
    }
  }, []);

  const replaySegment = useCallback(async (index: number) => {
    setError(null);
    setReplayingIndex(index);
    try {
      await invoke("replay_session_segment", { index });
    } catch (e) {
      setError(String(e));
    } finally {
      setReplayingIndex(null);
    }
  }, []);

  const pct = Math.round(Math.min(1, level) * 100);

  return (
    <main className="container">
      <h1>local-dictate</h1>
      <p className="subtitle">
        VAD → local Whisper · new segments copy to clipboard · ⌘⇧V pastes into focused app
      </p>

      {!micAuthorized && (
        <div className="permission-block">
          <p className="hint">
            Microphone: <strong>{micStatus || "…"}</strong>
            {micStatus === "denied" && " — enable Cursor/Terminal or this app in System Settings."}
          </p>
          <button type="button" onClick={openSettings}>
            Open microphone settings
          </button>
        </div>
      )}

      {!axAuthorized && (
        <div className="permission-block">
          <p className="hint">
            Accessibility: <strong>{axStatus || "…"}</strong>
            {axStatus === "denied" &&
              " — required to paste into TextEdit, Notes, etc. Enable local-dictate in Accessibility."}
          </p>
          <button type="button" onClick={openAxSettings}>
            Open accessibility settings
          </button>
        </div>
      )}

      <section className="paste-block" aria-label="Paste last transcript">
        <p className="hint">
          Latest transcript:{" "}
          <strong>{latestTranscript ? latestTranscript.slice(0, 120) : "none yet"}</strong>
        </p>
        <button
          type="button"
          disabled={pasteBusy || !latestTranscript}
          onClick={() => void pasteLast()}
        >
          {pasteBusy ? "Pasting…" : "Paste latest (⌘⇧V) — or ⌘V after a new transcript"}
        </button>
      </section>

      {devices.length > 0 && (
        <label className="device-select">
          <span className="device-select-label">Input</span>
          <select
            value={selectedDevice}
            disabled={monitoring}
            onChange={(e) => setSelectedDevice(e.target.value)}
          >
            {devices.map((d) => (
              <option key={d.name} value={d.name}>
                {d.name}
                {d.isDefault ? " (system default)" : ""}
              </option>
            ))}
          </select>
        </label>
      )}

      {appSettings && (
        <section className="settings-block" aria-label="App settings">
          <p className="device-select-label">Settings</p>
          <label className="settings-row">
            <span>
              VAD sensitivity ({appSettings.vadRmsThreshold.toFixed(3)})
            </span>
            <input
              type="range"
              min={0.002}
              max={0.05}
              step={0.001}
              value={appSettings.vadRmsThreshold}
              disabled={monitoring}
              onChange={(e) => void updateVadThreshold(Number(e.target.value))}
            />
          </label>
          {monitoring && (
            <p className="hint">VAD threshold applies on the next monitoring start.</p>
          )}
          <label className="settings-row">
            <span>Whisper language</span>
            <select
              value={appSettings.whisperLanguage}
              onChange={(e) => void updateWhisperLanguage(e.target.value as WhisperLanguage)}
            >
              <option value="auto">Auto</option>
              <option value="de">German</option>
              <option value="en">English</option>
            </select>
          </label>
        </section>
      )}

      <div className="meter" aria-label="Microphone level">
        <div className="meter-fill" style={{ width: `${pct}%` }} />
      </div>
      <p className="meter-label">{pct}%</p>

      <div className="row actions">
        {!monitoring ? (
          <button type="button" onClick={() => void start()}>
            Start monitoring
          </button>
        ) : (
          <button type="button" onClick={() => void stop()}>
            Stop
          </button>
        )}
      </div>

      {info && (
        <p className="device-info">
          {info.deviceName}
          <br />
          {info.sampleRate} Hz · {info.channels} ch · {info.sampleFormat}
        </p>
      )}

      {monitoring && (
        <section className="live-stt" aria-live="polite">
          <p className="segment-info">
            Segments: {segmentCount}
            {sttPendingIndex !== null && transcripts[sttPendingIndex] === undefined && (
              <> · transcribing #{sttPendingIndex + 1}…</>
            )}
            {lastSegment && (
              <>
                <br />
                Last: {lastSegment.durationSecs.toFixed(2)} s · {lastSegment.sampleCount} samples @{" "}
                {lastSegment.sampleRate} Hz
              </>
            )}
          </p>
          {Object.keys(transcripts).length > 0 && (
            <ul className="live-transcript-list">
              {Object.entries(transcripts)
                .map(([idx, text]) => ({ index: Number(idx), text }))
                .sort((a, b) => a.index - b.index)
                .map((row) => (
                  <li key={row.index}>
                    <span className="live-transcript-label">#{row.index + 1}</span> {row.text}
                  </li>
                ))}
            </ul>
          )}
        </section>
      )}

      {whisperModel && !whisperModel.exists && (
        <p className="hint">
          Whisper model missing — run <code>pnpm download-model</code>
        </p>
      )}

      {!monitoring && sessionSegments.length > 0 && (
        <section className="segment-replay" aria-label="Session segments">
          <p className="segment-replay-title">Session segments (16 kHz)</p>
          <ul className="segment-list">
            {sessionSegments.map((seg) => (
              <li key={seg.index}>
                <div className="segment-list-main">
                  <span className="segment-list-label">
                    #{seg.index + 1} — {seg.durationSecs.toFixed(2)} s
                  </span>
                  {transcripts[seg.index] && (
                    <p className="segment-transcript">{transcripts[seg.index]}</p>
                  )}
                </div>
                <span className="segment-list-actions">
                  <button
                    type="button"
                    disabled={replayingIndex !== null || transcribingIndex !== null}
                    onClick={() => void replaySegment(seg.index)}
                  >
                    {replayingIndex === seg.index ? "Playing…" : "Replay"}
                  </button>
                  <button
                    type="button"
                    disabled={
                      replayingIndex !== null ||
                      transcribingIndex !== null ||
                      !whisperModel?.exists
                    }
                    onClick={() => void transcribeSegment(seg.index)}
                  >
                    {transcribingIndex === seg.index ? "STT…" : "Transcribe"}
                  </button>
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {error && <p className="error">{error}</p>}
    </main>
  );
}

export default App;
