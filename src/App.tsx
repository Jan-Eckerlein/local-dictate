import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  dismissAppToast,
  notifyFromRaw,
  showAppToast,
  TOAST_ID,
} from "@/lib/app-toast";
import { buildAppStatus, deriveUiPhase } from "@/lib/ui-phase";
import { Toaster } from "@/components/ui/sonner";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Slider } from "@/components/ui/slider";
import { ThemeSwitcher } from "@/components/kibo-ui/theme-switcher";
import { useTheme } from "@/hooks/use-theme";

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
  filename: string;
  path: string;
  exists: boolean;
  loaded: boolean;
  envOverride: boolean;
}

interface WhisperModelOption {
  filename: string;
  label: string;
  exists: boolean;
  path: string;
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

const WHISPER_LANGUAGE_ITEMS: { value: WhisperLanguage; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "de", label: "German" },
  { value: "en", label: "English" },
];

interface AppSettings {
  vadRmsThreshold: number;
  whisperLanguage: WhisperLanguage;
  whisperModel: string;
}

function App() {
  const [monitoring, setMonitoring] = useState(false);
  const [level, setLevel] = useState(0);
  const [info, setInfo] = useState<AudioMonitorInfo | null>(null);
  const [micStatus, setMicStatus] = useState("");
  const [devices, setDevices] = useState<InputDeviceInfo[]>([]);
  const [selectedDevice, setSelectedDevice] = useState("");
  const [lastSegment, setLastSegment] = useState<AudioSegmentPayload | null>(
    null,
  );
  const [segmentCount, setSegmentCount] = useState(0);
  const [sessionSegments, setSessionSegments] = useState<
    SessionSegmentSummary[]
  >([]);
  const [replayingIndex, setReplayingIndex] = useState<number | null>(null);
  const [transcribingIndex, setTranscribingIndex] = useState<number | null>(
    null,
  );
  const [transcripts, setTranscripts] = useState<Record<number, string>>({});
  const [whisperModel, setWhisperModel] = useState<WhisperModelInfo | null>(
    null,
  );
  const [whisperModelOptions, setWhisperModelOptions] = useState<
    WhisperModelOption[]
  >([]);
  const [sttPendingIndex, setSttPendingIndex] = useState<number | null>(null);
  const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
  const [axStatus, setAxStatus] = useState("");
  const [latestTranscript, setLatestTranscript] = useState<string | null>(null);
  const [pasteBusy, setPasteBusy] = useState(false);
  const { theme, setTheme } = useTheme();

  const refreshMicStatus = useCallback(() => {
    void invoke<string>("mic_permission_status").then(setMicStatus);
  }, []);

  const refreshAxStatus = useCallback(() => {
    void invoke<string>("accessibility_permission_status").then(setAxStatus);
  }, []);

  const refreshLatestTranscript = useCallback(() => {
    void invoke<string | null>("get_latest_transcript").then(
      setLatestTranscript,
    );
  }, []);

  const refreshWhisperModel = useCallback(() => {
    void invoke<WhisperModelInfo>("whisper_model_info").then(setWhisperModel);
    void invoke<WhisperModelOption[]>("list_whisper_models").then(
      setWhisperModelOptions,
    );
  }, []);

  useEffect(() => {
    refreshMicStatus();
    refreshAxStatus();
    refreshLatestTranscript();
    refreshWhisperModel();
    void invoke<AppSettings>("get_app_settings").then(setAppSettings);
    void invoke<string>("request_microphone_permission").then(setMicStatus);
  }, [
    refreshMicStatus,
    refreshAxStatus,
    refreshLatestTranscript,
    refreshWhisperModel,
  ]);

  // Keep mic / Accessibility TCC in sync with System Settings (toast can clear).
  useEffect(() => {
    const id = window.setInterval(() => {
      refreshMicStatus();
      refreshAxStatus();
      refreshWhisperModel();
    }, 1500);
    return () => window.clearInterval(id);
  }, [refreshMicStatus, refreshAxStatus, refreshWhisperModel]);

  const updateVadThreshold = useCallback(async (vadRmsThreshold: number) => {
    try {
      const next = await invoke<AppSettings>("update_app_settings", {
        vadRmsThreshold,
      });
      setAppSettings(next);
    } catch (e) {
      notifyFromRaw(String(e));
    }
  }, []);

  const updateWhisperLanguage = useCallback(
    async (whisperLanguage: WhisperLanguage) => {
      try {
        const next = await invoke<AppSettings>("update_app_settings", {
          whisperLanguage,
        });
        setAppSettings(next);
      } catch (e) {
        notifyFromRaw(String(e));
      }
    },
    [],
  );

  const updateWhisperModel = useCallback(async (whisperModel: string) => {
    try {
      const next = await invoke<AppSettings>("update_app_settings", {
        whisperModel,
      });
      setAppSettings(next);
      refreshWhisperModel();
    } catch (e) {
      notifyFromRaw(String(e));
    }
  }, [refreshWhisperModel]);

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
      notifyFromRaw(String(e));
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
    void (async () => {
      unlistenLevel = await listen<AudioLevelPayload>(
        "audio-level",
        (event) => {
          setLevel(event.payload.level);
        },
      );
      if (cancelled) {
        await unlistenLevel();
        return;
      }

      unlistenSegment = await listen<AudioSegmentPayload>(
        "audio-segment",
        (event) => {
          setLastSegment(event.payload);
          setSegmentCount(event.payload.index + 1);
          setSttPendingIndex(event.payload.index);
        },
      );
      if (cancelled) {
        await unlistenSegment();
        return;
      }

      unlistenTranscript = await listen<TranscriptReadyPayload>(
        "transcript-ready",
        (event) => {
          const { segmentIndex, text } = event.payload;
          setTranscripts((prev) => ({ ...prev, [segmentIndex]: text }));
          setLatestTranscript(text.trim() || null);
          setSttPendingIndex((pending) =>
            pending === segmentIndex ? null : pending,
          );
          refreshWhisperModel();
        },
      );
      if (cancelled) {
        await unlistenTranscript();
        return;
      }

      unlistenTranscriptErr = await listen<TranscriptErrorPayload>(
        "transcript-error",
        (event) => {
          showAppToast(
            `STT #${event.payload.segmentIndex + 1}: ${event.payload.message}`,
            "error",
          );
          setSttPendingIndex((pending) =>
            pending === event.payload.segmentIndex ? null : pending,
          );
        },
      );
      if (cancelled) {
        await unlistenTranscriptErr();
        return;
      }

      unlistenPasteErr = await listen<string>("paste-error", (event) => {
        notifyFromRaw(event.payload);
      });
      if (cancelled) {
        await unlistenPasteErr();
      }
    })();

    return () => {
      cancelled = true;
      void unlistenLevel?.();
      void unlistenSegment?.();
      void unlistenTranscript?.();
      void unlistenTranscriptErr?.();
      void unlistenPasteErr?.();
    };
  }, [refreshWhisperModel]);

  const openSettings = useCallback(() => {
    void invoke("open_microphone_settings").then(refreshMicStatus);
  }, [refreshMicStatus]);

  const openAxSettings = useCallback(() => {
    void invoke("open_accessibility_settings").then(refreshAxStatus);
  }, [refreshAxStatus]);

  const runPaste = useCallback(
    async (
      command:
        | "paste_last_transcript"
        | "paste_since_last_copy"
        | "paste_all_transcripts",
    ) => {
      setPasteBusy(true);
      try {
        await invoke(command);
      } catch (e) {
        notifyFromRaw(String(e));
      } finally {
        setPasteBusy(false);
        refreshAxStatus();
      }
    },
    [refreshAxStatus],
  );

  const start = useCallback(async () => {
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
      setLatestTranscript(null);
      setMonitoring(true);
      refreshMicStatus();
    } catch (e) {
      setMonitoring(false);
      notifyFromRaw(String(e));
      refreshMicStatus();
    }
  }, [refreshMicStatus, selectedDevice]);

  const stop = useCallback(async () => {
    try {
      await invoke("stop_audio_monitor");
      setMonitoring(false);
      setLevel(0);
      const list = await invoke<SessionSegmentSummary[]>(
        "list_session_segments",
      );
      setSessionSegments(list);
    } catch (e) {
      notifyFromRaw(String(e));
    }
  }, []);

  const transcribeSegment = useCallback(async (index: number) => {
    setTranscribingIndex(index);
    try {
      const result = await invoke<TranscriptResult>(
        "transcribe_session_segment",
        { index },
      );
      setTranscripts((prev) => ({
        ...prev,
        [result.segmentIndex]: result.text,
      }));
      refreshWhisperModel();
    } catch (e) {
      notifyFromRaw(String(e));
    } finally {
      setTranscribingIndex(null);
    }
  }, [refreshWhisperModel]);

  const replaySegment = useCallback(async (index: number) => {
    setReplayingIndex(index);
    try {
      await invoke("replay_session_segment", { index });
    } catch (e) {
      notifyFromRaw(String(e));
    } finally {
      setReplayingIndex(null);
    }
  }, []);

  const pct = Math.round(Math.min(1, level) * 100);

  const axAuthorized = axStatus === "authorized";

  const uiPhase = useMemo(
    () =>
      deriveUiPhase({
        micStatus,
        monitoring,
        whisperExists: whisperModel?.exists ?? null,
        sttPendingIndex,
      }),
    [micStatus, monitoring, whisperModel?.exists, sttPendingIndex],
  );

  const status = useMemo(
    () =>
      buildAppStatus(uiPhase, {
        axAuthorized,
        hasTranscript: Boolean(latestTranscript),
        pasteBusy,
      }),
    [uiPhase, axAuthorized, latestTranscript, pasteBusy],
  );

  const whisperModelItems = useMemo(() => {
    const opts =
      whisperModelOptions.length > 0
        ? whisperModelOptions
        : [
            {
              filename: "ggml-base.bin",
              label: "Base (~141 MB)",
              exists: true,
              path: "",
            },
          ];
    return opts.map((opt) => ({
      value: opt.filename,
      label: `${opt.label}${!opt.exists ? " — not on disk" : ""}`,
    }));
  }, [whisperModelOptions]);

  useEffect(() => {
    if (whisperModel === null) return;
    if (whisperModel.exists) {
      dismissAppToast(TOAST_ID.model);
      return;
    }
    const hint = whisperModel.envOverride
      ? "LOCAL_DICTATE_WHISPER_MODEL is set — fix or unset that path."
      : `Download with WHISPER_MODEL=${whisperModel.filename} pnpm download-model (dev) or place the file under Application Support/models/.`;
    showAppToast(
      `Whisper model not found (${whisperModel.filename}). Expected at: ${whisperModel.path}. ${hint}`,
      "error",
      { id: TOAST_ID.model },
    );
  }, [whisperModel]);

  useEffect(() => {
    if (micStatus === "" || micStatus === "authorized") {
      dismissAppToast(TOAST_ID.mic);
      return;
    }
    const message =
      micStatus === "denied"
        ? "Microphone denied — enable this app under System Settings → Privacy & Security → Microphone."
        : "Microphone access is required to start monitoring.";
    showAppToast(message, "error", {
      id: TOAST_ID.mic,
      action: {
        label: "Settings",
        onClick: () => void openSettings(),
      },
    });
  }, [micStatus, openSettings]);

  useEffect(() => {
    if (axStatus === "" || axStatus === "authorized") {
      dismissAppToast(TOAST_ID.ax);
      return;
    }
    showAppToast(
      "Accessibility is required for paste shortcuts (⌘⇧V / B / A). Enable this app under System Settings → Privacy & Security → Accessibility.",
      "error",
      {
        id: TOAST_ID.ax,
        action: {
          label: "Settings",
          onClick: () => void openAxSettings(),
        },
      },
    );
  }, [axStatus, openAxSettings]);

  return (
    <div className="relative min-h-screen">
      <Toaster themeMode={theme} />
      <div className="absolute top-4 right-4 z-10">
        <ThemeSwitcher value={theme} onChange={setTheme} />
      </div>
      <main className="mx-auto flex w-full max-w-lg flex-col gap-4 p-4 pb-8">
        <header className="space-y-1 pr-28">
          <div className="flex flex-wrap items-baseline gap-2">
            <h1 className="text-2xl font-semibold tracking-tight">
              local-dictate
            </h1>
            <span className="text-muted-foreground text-xs font-medium">
              {status.label}
            </span>
          </div>
          <p className="text-muted-foreground text-sm">
            VAD → local Whisper · ⌘⇧V last · ⌘⇧B batch · ⌘⇧A all
          </p>
        </header>

      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-base">Paste</CardTitle>
          <CardDescription>
            Latest:{" "}
            <span className="text-foreground font-medium">
              {latestTranscript ? latestTranscript.slice(0, 120) : "none yet"}
            </span>
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            disabled={!status.canPaste}
            onClick={() => void runPaste("paste_last_transcript")}
          >
            Last (⌘⇧V)
          </Button>
          <Button
            type="button"
            variant="secondary"
            size="sm"
            disabled={!status.canPaste}
            onClick={() => void runPaste("paste_since_last_copy")}
          >
            Batch (⌘⇧B)
          </Button>
          <Button
            type="button"
            variant="secondary"
            size="sm"
            disabled={!status.canPaste}
            onClick={() => void runPaste("paste_all_transcripts")}
          >
            All (⌘⇧A)
          </Button>
        </CardContent>
      </Card>

      {devices.length > 0 && (
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-base">Input</CardTitle>
          </CardHeader>
          <CardContent>
            <Select
              value={selectedDevice}
              onValueChange={(v) => {
                if (v) setSelectedDevice(v);
              }}
              disabled={monitoring}
            >
              <SelectTrigger className="w-full">
                <SelectValue placeholder="Select microphone" />
              </SelectTrigger>
              <SelectContent>
                {devices.map((d) => (
                  <SelectItem key={d.name} value={d.name}>
                    {d.name}
                    {d.isDefault ? " (default)" : ""}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </CardContent>
        </Card>
      )}

      {appSettings && (
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-base">Settings</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <Label>
                VAD sensitivity ({appSettings.vadRmsThreshold.toFixed(3)})
              </Label>
              <Slider
                min={0.002}
                max={0.05}
                step={0.001}
                disabled={monitoring}
                value={[appSettings.vadRmsThreshold]}
                onValueChange={(v) => {
                  const next = Array.isArray(v) ? v[0] : v;
                  void updateVadThreshold(next ?? 0.008);
                }}
              />
              {monitoring && (
                <p className="text-muted-foreground text-xs">
                  VAD threshold applies on the next monitoring start.
                </p>
              )}
            </div>
            <div className="space-y-2">
              <Label htmlFor="whisper-model">Whisper model</Label>
              <Select
                items={whisperModelItems}
                value={appSettings.whisperModel}
                disabled={whisperModel?.envOverride}
                onValueChange={(v) => {
                  if (v) void updateWhisperModel(v);
                }}
              >
                <SelectTrigger id="whisper-model" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {whisperModelItems.map((item) => (
                    <SelectItem key={item.value} value={item.value}>
                      {item.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {whisperModel && (
                <p className="text-muted-foreground text-xs wrap-break-word">
                  {whisperModel.envOverride
                    ? "Model path overridden by LOCAL_DICTATE_WHISPER_MODEL."
                    : whisperModel.exists
                      ? whisperModel.loaded
                        ? `Loaded from ${whisperModel.path}`
                        : `Ready at ${whisperModel.path} (loads on next segment)`
                      : `Missing — run WHISPER_MODEL=${whisperModel.filename} pnpm download-model`}
                </p>
              )}
            </div>
            <div className="space-y-2">
              <Label htmlFor="whisper-lang">Whisper language</Label>
              <Select
                items={WHISPER_LANGUAGE_ITEMS}
                value={appSettings.whisperLanguage}
                onValueChange={(v) =>
                  void updateWhisperLanguage(v as WhisperLanguage)
                }
              >
                <SelectTrigger id="whisper-lang" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {WHISPER_LANGUAGE_ITEMS.map((item) => (
                    <SelectItem key={item.value} value={item.value}>
                      {item.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </CardContent>
        </Card>
      )}

      <div className="space-y-1">
        <div
          className="bg-muted h-2 w-full overflow-hidden rounded-full"
          aria-label="Microphone level"
        >
          <div
            className="bg-primary h-full transition-[width] duration-75"
            style={{ width: `${pct}%` }}
          />
        </div>
        <p className="text-muted-foreground text-center text-xs">{pct}%</p>
      </div>

      <Button
        type="button"
        className="w-full"
        size="lg"
        variant={monitoring ? "secondary" : "default"}
        disabled={!monitoring && !status.canStartMonitoring}
        onClick={() => void (monitoring ? stop() : start())}
      >
        {monitoring ? "Stop monitoring" : "Start monitoring"}
      </Button>

      {info && (
        <p className="text-muted-foreground text-center text-xs">
          {info.deviceName} · {info.sampleRate} Hz · {info.channels} ch ·{" "}
          {info.sampleFormat}
        </p>
      )}

      {monitoring && (
        <Card aria-live="polite">
          <CardHeader className="pb-2">
            <CardTitle className="text-base">Live transcript</CardTitle>
            <CardDescription>
              Segments: {segmentCount}
              {sttPendingIndex !== null &&
                transcripts[sttPendingIndex] === undefined && (
                  <> · transcribing #{sttPendingIndex + 1}…</>
                )}
              {lastSegment && (
                <> · last {lastSegment.durationSecs.toFixed(2)} s</>
              )}
            </CardDescription>
          </CardHeader>
          {Object.keys(transcripts).length > 0 && (
            <CardContent className="space-y-2">
              {Object.entries(transcripts)
                .map(([idx, text]) => ({ index: Number(idx), text }))
                .sort((a, b) => a.index - b.index)
                .map((row) => (
                  <p key={row.index} className="text-sm leading-snug">
                    <span className="text-muted-foreground">
                      #{row.index + 1}
                    </span>{" "}
                    {row.text}
                  </p>
                ))}
            </CardContent>
          )}
        </Card>
      )}

      {!monitoring && sessionSegments.length > 0 && (
        <Card aria-label="Session segments">
          <CardHeader className="pb-2">
            <CardTitle className="text-base">
              Session segments (debug)
            </CardTitle>
            <CardDescription>
              16 kHz · replay / manual transcribe
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {sessionSegments.map((seg) => (
              <div key={seg.index} className="space-y-2">
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <h3 className="text-sm font-medium">
                    #{seg.index + 1} — {seg.durationSecs.toFixed(2)} s
                  </h3>
                  <div className="flex gap-2">
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={
                        replayingIndex !== null || transcribingIndex !== null
                      }
                      onClick={() => void replaySegment(seg.index)}
                    >
                      {replayingIndex === seg.index ? "Playing…" : "Replay"}
                    </Button>
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={
                        replayingIndex !== null ||
                        transcribingIndex !== null ||
                        !whisperModel?.exists
                      }
                      onClick={() => void transcribeSegment(seg.index)}
                    >
                      {transcribingIndex === seg.index ? "STT…" : "Transcribe"}
                    </Button>
                  </div>
                </div>
                {transcripts[seg.index] && (
                  <p className="text-muted-foreground text-sm">
                    {transcripts[seg.index]}
                  </p>
                )}
                {seg.index < sessionSegments.length - 1 && <Separator />}
              </div>
            ))}
          </CardContent>
        </Card>
      )}
      </main>
    </div>
  );
}

export default App;
