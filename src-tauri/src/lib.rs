mod audio;
mod macos_mic;

use audio::{list_input_devices, start_monitor, stop_monitor, AudioMonitorInfo, InputDeviceInfo};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tauri::State;

struct AudioMonitorState {
    stop: Option<Arc<AtomicBool>>,
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
fn start_audio_monitor(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AudioMonitorState>>,
    device_name: Option<String>,
) -> Result<AudioMonitorInfo, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "audio state lock poisoned".to_string())?;

    if guard.stop.is_some() {
        return Err("already monitoring".into());
    }

    macos_mic::ensure_access()?;

    let stop = Arc::new(AtomicBool::new(false));
    let info = start_monitor(app, stop.clone(), device_name)?;
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
        .manage(Mutex::new(AudioMonitorState { stop: None }))
        .invoke_handler(tauri::generate_handler![
            mic_permission_status,
            open_microphone_settings,
            list_input_audio_devices,
            start_audio_monitor,
            stop_audio_monitor,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
