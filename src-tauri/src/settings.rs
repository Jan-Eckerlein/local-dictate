use crate::segment::VAD_RMS_THRESHOLD;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhisperModelFile {
    Tiny,
    Base,
    Small,
}

impl WhisperModelFile {
    pub const ALL: [Self; 3] = [Self::Tiny, Self::Base, Self::Small];

    pub fn filename(self) -> &'static str {
        match self {
            Self::Tiny => "ggml-tiny.bin",
            Self::Base => "ggml-base.bin",
            Self::Small => "ggml-small.bin",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Tiny => "Tiny (~75 MB)",
            Self::Base => "Base (~141 MB)",
            Self::Small => "Small (~465 MB)",
        }
    }

    pub fn from_filename(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|m| m.filename() == name)
    }
}

impl Default for WhisperModelFile {
    fn default() -> Self {
        Self::Base
    }
}

impl Serialize for WhisperModelFile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.filename())
    }
}

impl<'de> Deserialize<'de> for WhisperModelFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::from_filename(&raw).ok_or_else(|| {
            serde::de::Error::custom(format!("unknown whisper model file: {raw}"))
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub vad_rms_threshold: f32,
    pub whisper_language: WhisperLanguage,
    #[serde(default)]
    pub whisper_model: WhisperModelFile,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            vad_rms_threshold: VAD_RMS_THRESHOLD,
            whisper_language: WhisperLanguage::De,
            whisper_model: WhisperModelFile::Base,
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
        whisper_model: Option<WhisperModelFile>,
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
        if let Some(model) = whisper_model {
            guard.whisper_model = model;
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
