use crate::settings::SettingsState;
use crate::stt::SttState;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;
use tauri::{AppHandle, Emitter};

pub struct SttJob {
    pub index: usize,
    pub samples: Vec<f32>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptReadyPayload {
    pub segment_index: usize,
    pub text: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptErrorPayload {
    pub segment_index: usize,
    pub message: String,
}

pub fn spawn_stt_worker(
    app: AppHandle,
    stt: Arc<SttState>,
    settings: SettingsState,
    rx: Receiver<SttJob>,
) {
    thread::spawn(move || {
        while let Ok(job) = rx.recv() {
            match stt.transcribe(&job.samples, &settings) {
                Ok(text) => {
                    let payload = TranscriptReadyPayload {
                        segment_index: job.index,
                        text,
                    };
                    let _ = app.emit("transcript-ready", payload);
                }
                Err(message) => {
                    let payload = TranscriptErrorPayload {
                        segment_index: job.index,
                        message,
                    };
                    let _ = app.emit("transcript-error", payload);
                }
            }
        }
    });
}
