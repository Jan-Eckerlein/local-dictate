use crate::segment::VAD_RMS_THRESHOLD;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const VAD_RMS_MIN: f32 = 0.002;
const VAD_RMS_MAX: f32 = 0.05;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WhisperLanguage {
    Auto,
    De,
    En,
}

impl WhisperLanguage {
    pub fn as_whisper_lang(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::De => Some("de"),
            Self::En => Some("en"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub vad_rms_threshold: f32,
    pub whisper_language: WhisperLanguage,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            vad_rms_threshold: VAD_RMS_THRESHOLD,
            whisper_language: WhisperLanguage::Auto,
        }
    }
}

#[derive(Clone)]
pub struct SettingsState {
    path: PathBuf,
    inner: ArcSettings,
}

type ArcSettings = std::sync::Arc<Mutex<AppSettings>>;

impl SettingsState {
    pub fn load(app: &AppHandle) -> Result<Self, String> {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("app data dir: {e}"))?;
        fs::create_dir_all(&dir).map_err(|e| format!("create app data dir: {e}"))?;
        let path = dir.join("settings.json");

        let settings = if path.exists() {
            let raw = fs::read_to_string(&path).map_err(|e| format!("read settings: {e}"))?;
            serde_json::from_str(&raw).unwrap_or_else(|_| AppSettings::default())
        } else {
            AppSettings::default()
        };

        let settings = normalize(settings);
        let state = Self {
            path: path.clone(),
            inner: std::sync::Arc::new(Mutex::new(settings)),
        };
        state.save()?;
        Ok(state)
    }

    pub fn get(&self) -> Result<AppSettings, String> {
        self.inner
            .lock()
            .map(|g| g.clone())
            .map_err(|_| "settings lock poisoned".to_string())
    }

    pub fn update(
        &self,
        vad_rms_threshold: Option<f32>,
        whisper_language: Option<WhisperLanguage>,
    ) -> Result<AppSettings, String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        if let Some(t) = vad_rms_threshold {
            guard.vad_rms_threshold = clamp_vad_rms(t);
        }
        if let Some(lang) = whisper_language {
            guard.whisper_language = lang;
        }
        *guard = normalize(guard.clone());
        drop(guard);
        self.save()?;
        self.get()
    }

    fn save(&self) -> Result<(), String> {
        let settings = self.get()?;
        let json = serde_json::to_string_pretty(&settings).map_err(|e| format!("serialize: {e}"))?;
        fs::write(&self.path, json).map_err(|e| format!("write settings: {e}"))
    }
}

fn normalize(mut s: AppSettings) -> AppSettings {
    s.vad_rms_threshold = clamp_vad_rms(s.vad_rms_threshold);
    s
}

fn clamp_vad_rms(value: f32) -> f32 {
    if value.is_nan() {
        return VAD_RMS_THRESHOLD;
    }
    value.clamp(VAD_RMS_MIN, VAD_RMS_MAX)
}
