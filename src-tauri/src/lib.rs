mod audio;
mod macos_accessibility;
mod macos_mic;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod paste;
mod replay;
mod segment;
mod settings;
mod stt;
mod stt_worker;
mod transcript_buffer;

use audio::{list_input_devices, start_monitor, stop_monitor, AudioMonitorInfo, InputDeviceInfo};
use segment::AudioSegmentPayload;
use settings::{AppSettings, SettingsState, WhisperLanguage};
use stt::{SttState, WhisperModelInfo};
use stt_worker::{spawn_stt_worker, SttJob};
use transcript_buffer::TranscriptBuffer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};

pub const PASTE_SHORTCUT: &str = "CommandOrControl+Shift+V";

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptResult {
    segment_index: usize,
    text: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionSegmentSummary {
    index: usize,
    sample_rate: u32,
    sample_count: usize,
    duration_secs: f32,
}

struct AudioMonitorState {
    stop: Option<Arc<AtomicBool>>,
    session_segments: Arc<Mutex<Vec<Vec<f32>>>>,
    replaying: Arc<AtomicBool>,
}

impl AudioMonitorState {
    fn new() -> Self {
        Self {
            stop: None,
            session_segments: Arc::new(Mutex::new(Vec::new())),
            replaying: Arc::new(AtomicBool::new(false)),
        }
    }
}

struct SttInbox {
    tx: mpsc::Sender<SttJob>,
}

#[tauri::command]
fn mic_permission_status() -> String {
    macos_mic::authorization_status().to_string()
}

#[tauri::command]
fn open_microphone_settings() -> Result<(), String> {
    macos_mic::open_privacy_settings()
}

#[tauri::command]
fn accessibility_permission_status() -> String {
    macos_accessibility::authorization_status().to_string()
}

#[tauri::command]
fn open_accessibility_settings() -> Result<(), String> {
    macos_accessibility::open_privacy_settings()
}

#[tauri::command]
fn get_latest_transcript(buffer: State<'_, TranscriptBuffer>) -> Result<Option<String>, String> {
    buffer.latest()
}

#[tauri::command]
fn paste_last_transcript(buffer: State<'_, TranscriptBuffer>) -> Result<(), String> {
    macos_accessibility::ensure_trusted()?;
    let text = buffer.take_for_paste()?;
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        paste::paste_text(&text)?;
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = text;
        return Err("paste not supported on this platform".into());
    }
    Ok(())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn paste_from_shortcut(app: &tauri::AppHandle) {
    let buffer = app.state::<TranscriptBuffer>();
    match macos_accessibility::ensure_trusted() {
        Ok(()) => {}
        Err(message) => {
            let _ = app.emit("paste-error", message);
            return;
        }
    }
    let text = match buffer.take_for_paste() {
        Ok(t) => t,
        Err(message) => {
            let _ = app.emit("paste-error", message);
            return;
        }
    };
    match paste::paste_text(&text) {
        Ok(()) => {
            let _ = app.emit("paste-done", ());
        }
        Err(message) => {
            let _ = app.emit("paste-error", message);
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn register_paste_shortcut(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    app.global_shortcut()
        .on_shortcut(PASTE_SHORTCUT, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                paste_from_shortcut(app);
            }
        })
        .map_err(|e| format!("register paste shortcut: {e}"))
}

#[tauri::command]
fn list_input_audio_devices() -> Result<Vec<InputDeviceInfo>, String> {
    list_input_devices()
}

#[tauri::command]
fn get_app_settings(settings: State<'_, SettingsState>) -> Result<AppSettings, String> {
    settings.get()
}

#[tauri::command]
fn update_app_settings(
    vad_rms_threshold: Option<f32>,
    whisper_language: Option<WhisperLanguage>,
    settings: State<'_, SettingsState>,
) -> Result<AppSettings, String> {
    settings.update(vad_rms_threshold, whisper_language)
}

#[tauri::command]
fn list_session_segments(
    state: State<'_, Mutex<AudioMonitorState>>,
) -> Result<Vec<SessionSegmentSummary>, String> {
    let guard = state
        .lock()
        .map_err(|_| "audio state lock poisoned".to_string())?;
    Ok(guard
        .session_segments
        .lock()
        .map_err(|_| "session segments lock poisoned".to_string())?
        .iter()
        .enumerate()
        .map(|(index, seg)| {
            let summary = AudioSegmentPayload::from_segment(seg, index);
            SessionSegmentSummary {
                index: summary.index,
                sample_rate: summary.sample_rate,
                sample_count: summary.sample_count,
                duration_secs: summary.duration_secs,
            }
        })
        .collect())
}

#[tauri::command]
fn whisper_model_info(stt: State<'_, Arc<SttState>>) -> WhisperModelInfo {
    stt.model_info()
}

/// Manual STT (debug); auto pipeline uses `transcript-ready` events while monitoring.
#[tauri::command]
fn transcribe_session_segment(
    index: usize,
    state: State<'_, Mutex<AudioMonitorState>>,
    stt: State<'_, Arc<SttState>>,
    settings: State<'_, SettingsState>,
    buffer: State<'_, TranscriptBuffer>,
) -> Result<TranscriptResult, String> {
    let guard = state
        .lock()
        .map_err(|_| "audio state lock poisoned".to_string())?;

    if guard.stop.is_some() {
        return Err("stop monitoring before manual transcribe".into());
    }

    let samples = {
        let segments = guard
            .session_segments
            .lock()
            .map_err(|_| "session segments lock poisoned".to_string())?;
        segments
            .get(index)
            .cloned()
            .ok_or_else(|| format!("segment index out of range: {index}"))?
    };

    let text = stt.transcribe(&samples, &settings)?;
    buffer.set_latest(text.clone());
    Ok(TranscriptResult {
        segment_index: index,
        text,
    })
}

#[tauri::command]
fn replay_session_segment(
    index: usize,
    state: State<'_, Mutex<AudioMonitorState>>,
) -> Result<(), String> {
    let guard = state
        .lock()
        .map_err(|_| "audio state lock poisoned".to_string())?;

    if guard.stop.is_some() {
        return Err("stop monitoring before replay".into());
    }
    if guard.replaying.swap(true, Ordering::Relaxed) {
        return Err("replay already in progress".into());
    }

    let samples = {
        let segments = guard
            .session_segments
            .lock()
            .map_err(|_| "session segments lock poisoned".to_string())?;
        segments
            .get(index)
            .cloned()
            .ok_or_else(|| format!("segment index out of range: {index}"))?
    };

    let replaying = guard.replaying.clone();
    drop(guard);

    let result = replay::replay_samples_16k_mono(&samples);
    replaying.store(false, Ordering::Relaxed);
    result
}

#[tauri::command]
fn start_audio_monitor(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AudioMonitorState>>,
    inbox: State<'_, SttInbox>,
    settings: State<'_, SettingsState>,
    device_name: Option<String>,
) -> Result<AudioMonitorInfo, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "audio state lock poisoned".to_string())?;

    if guard.stop.is_some() {
        return Err("already monitoring".into());
    }
    if guard.replaying.load(Ordering::Relaxed) {
        return Err("wait for replay to finish".into());
    }

    macos_mic::ensure_access()?;

    guard
        .session_segments
        .lock()
        .map_err(|_| "session segments lock poisoned".to_string())?
        .clear();

    let vad_rms_threshold = settings.get()?.vad_rms_threshold;
    let stop = Arc::new(AtomicBool::new(false));
    let info = start_monitor(
        app,
        stop.clone(),
        device_name,
        guard.session_segments.clone(),
        inbox.tx.clone(),
        vad_rms_threshold,
    )?;
    guard.stop = Some(stop);
    Ok(info)
}

#[tauri::command]
fn stop_audio_monitor(state: State<'_, Mutex<AudioMonitorState>>) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|_| "audio state lock poisoned".to_string())?;

    if let Some(stop) = guard.stop.take() {
        stop_monitor(&stop);
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder.plugin(tauri_plugin_global_shortcut::Builder::new().build());
    }

    builder
        .setup(|app| {
            let (tx, rx) = mpsc::channel::<SttJob>();
            let stt = Arc::new(SttState::new());
            let settings = SettingsState::load(app.handle())?;
            let transcripts = TranscriptBuffer::default();
            spawn_stt_worker(
                app.handle().clone(),
                stt.clone(),
                settings.clone(),
                transcripts.clone(),
                rx,
            );
            app.manage(stt);
            app.manage(settings);
            app.manage(transcripts);
            app.manage(SttInbox { tx });
            app.manage(Mutex::new(AudioMonitorState::new()));

            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            register_paste_shortcut(app.handle())?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            mic_permission_status,
            open_microphone_settings,
            accessibility_permission_status,
            open_accessibility_settings,
            list_input_audio_devices,
            get_app_settings,
            update_app_settings,
            get_latest_transcript,
            paste_last_transcript,
            list_session_segments,
            whisper_model_info,
            transcribe_session_segment,
            replay_session_segment,
            start_audio_monitor,
            stop_audio_monitor,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
