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

function App() {
  const [monitoring, setMonitoring] = useState(false);
  const [level, setLevel] = useState(0);
  const [info, setInfo] = useState<AudioMonitorInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [micStatus, setMicStatus] = useState("");
  const [devices, setDevices] = useState<InputDeviceInfo[]>([]);
  const [selectedDevice, setSelectedDevice] = useState("");

  const refreshMicStatus = useCallback(() => {
    void invoke<string>("mic_permission_status").then(setMicStatus);
  }, []);

  useEffect(() => {
    refreshMicStatus();
  }, [refreshMicStatus]);

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
    let unlisten: UnlistenFn | undefined;

    listen<AudioLevelPayload>("audio-level", (event) => {
      setLevel(event.payload.level);
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      void unlisten?.();
    };
  }, []);

  const micAuthorized = micStatus === "authorized";

  const openSettings = useCallback(() => {
    void invoke("open_microphone_settings").then(refreshMicStatus);
  }, [refreshMicStatus]);

  const start = useCallback(async () => {
    setError(null);
    try {
      const deviceInfo = await invoke<AudioMonitorInfo>("start_audio_monitor", {
        deviceName: selectedDevice || null,
      });
      setInfo(deviceInfo);
      setMonitoring(true);
      refreshMicStatus();
    } catch (e) {
      setMonitoring(false);
      setError(String(e));
      refreshMicStatus();
    }
  }, [refreshMicStatus, selectedDevice]);

  const stop = useCallback(async () => {
    await invoke("stop_audio_monitor");
    setMonitoring(false);
    setLevel(0);
  }, []);

  const pct = Math.round(Math.min(1, level) * 100);

  return (
    <main className="container">
      <h1>local-dictate</h1>
      <p className="subtitle">Microphone level (cpal)</p>

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

      {error && <p className="error">{error}</p>}
    </main>
  );
}

export default App;
