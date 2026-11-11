mod audio;
mod macos_mic;
mod replay;
mod segment;
mod stt;
mod stt_worker;

use audio::{list_input_devices, start_monitor, stop_monitor, AudioMonitorInfo, InputDeviceInfo};
use segment::AudioSegmentPayload;
use stt::{SttState, WhisperModelInfo};
use stt_worker::{spawn_stt_worker, SttJob};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

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
fn list_input_audio_devices() -> Result<Vec<InputDeviceInfo>, String> {
    list_input_devices()
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

    let text = stt.transcribe(&samples)?;
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

    let stop = Arc::new(AtomicBool::new(false));
    let info = start_monitor(
        app,
        stop.clone(),
        device_name,
        guard.session_segments.clone(),
        inbox.tx.clone(),
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
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let (tx, rx) = mpsc::channel::<SttJob>();
            let stt = Arc::new(SttState::new());
            spawn_stt_worker(app.handle().clone(), stt.clone(), rx);
            app.manage(stt);
            app.manage(SttInbox { tx });
            app.manage(Mutex::new(AudioMonitorState::new()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            mic_permission_status,
            open_microphone_settings,
            list_input_audio_devices,
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
