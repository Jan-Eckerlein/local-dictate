use crate::segment::normalize_peak_for_stt;
use crate::settings::{SettingsState, WhisperLanguage, WhisperModelFile};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, path::BaseDirectory};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub const DEFAULT_MODEL_FILE: &str = "ggml-base.bin";

/// Override with `LOCAL_DICTATE_WHISPER_MODEL` (absolute path).
pub fn resolve_model_path(app: &AppHandle, filename: &str) -> PathBuf {
    if let Ok(path) = std::env::var("LOCAL_DICTATE_WHISPER_MODEL") {
        return PathBuf::from(path);
    }

    let mut candidates: Vec<PathBuf> = Vec::new();

    let resource_names = [
        format!("models/{filename}"),
        format!("_up_/models/{filename}"),
        filename.to_string(),
    ];
    for name in resource_names {
        if let Ok(p) = app.path().resolve(&name, BaseDirectory::Resource) {
            candidates.push(p);
        }
    }

    if let Ok(dir) = app.path().app_data_dir() {
        candidates.push(dir.join("models").join(filename));
    }

    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../models")
            .join(filename),
    );

    for p in &candidates {
        if p.exists() {
            return p.clone();
        }
    }

    app.path()
        .app_data_dir()
        .map(|dir| dir.join("models").join(filename))
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../models")
                .join(filename)
        })
}

pub fn whisper_model_env_override() -> bool {
    std::env::var("LOCAL_DICTATE_WHISPER_MODEL").is_ok()
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperModelInfo {
    pub filename: String,
    pub path: String,
    pub exists: bool,
    pub loaded: bool,
    pub env_override: bool,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperModelOption {
    pub filename: String,
    pub label: String,
    pub exists: bool,
    pub path: String,
}

pub fn list_whisper_model_options(app: &AppHandle) -> Vec<WhisperModelOption> {
    WhisperModelFile::ALL
        .iter()
        .map(|m| {
            let filename = m.filename();
            let path = resolve_model_path(app, filename);
            WhisperModelOption {
                filename: filename.to_string(),
                label: m.label().to_string(),
                exists: path.exists(),
                path: path.display().to_string(),
            }
        })
        .collect()
}

pub struct WhisperEngine {
    ctx: WhisperContext,
}

impl WhisperEngine {
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Err(format!(
                "model not found at {} — download with pnpm download-model (see README)",
                path.display()
            ));
        }
        let ctx = WhisperContext::new_with_params(
            path.to_str()
                .ok_or_else(|| "model path is not valid UTF-8".to_string())?,
            WhisperContextParameters::default(),
        )
        .map_err(|e| format!("load whisper model: {e}"))?;
        Ok(Self { ctx })
    }

    /// 16 kHz mono f32 PCM (same format as VAD segments).
    pub fn transcribe(&self, samples: &[f32], language: WhisperLanguage) -> Result<String, String> {
        if samples.is_empty() {
            return Err("audio is empty".into());
        }

        let samples = normalize_peak_for_stt(samples);

        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| format!("whisper state: {e}"))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(language.as_whisper_lang());
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_n_threads(
            std::thread::available_parallelism()
                .map(|n| n.get().min(6))
                .unwrap_or(4) as i32,
        );

        state
            .full(params, &samples)
            .map_err(|e| format!("whisper inference: {e}"))?;

        let mut text = String::new();
        for segment in state.as_iter() {
            let piece = segment.to_string();
            if !piece.is_empty() {
                if !text.is_empty() && !text.ends_with(' ') {
                    text.push(' ');
                }
                text.push_str(piece.trim());
            }
        }

        if text.is_empty() {
            return Err("no speech detected in segment".into());
        }
        Ok(text)
    }
}

pub struct SttState {
    app: AppHandle,
    active_path: Mutex<PathBuf>,
    engine: Mutex<Option<WhisperEngine>>,
}

impl SttState {
    pub fn new(app: &AppHandle) -> Self {
        let path = resolve_model_path(app, DEFAULT_MODEL_FILE);
        Self {
            app: app.clone(),
            active_path: Mutex::new(path),
            engine: Mutex::new(None),
        }
    }

    pub fn model_info(&self, settings: &SettingsState) -> Result<WhisperModelInfo, String> {
        let filename = settings.get()?.whisper_model.filename();
        let path = resolve_model_path(&self.app, filename);
        let loaded = self
            .engine
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false);
        let active = self
            .active_path
            .lock()
            .map(|p| *p == path)
            .unwrap_or(false);
        Ok(WhisperModelInfo {
            filename: filename.to_string(),
            path: path.display().to_string(),
            exists: path.exists(),
            loaded: loaded && active,
            env_override: whisper_model_env_override(),
        })
    }

    fn ensure_loaded(&self, settings: &SettingsState) -> Result<(), String> {
        let filename = settings.get()?.whisper_model.filename();
        let path = resolve_model_path(&self.app, filename);

        let mut active = self
            .active_path
            .lock()
            .map_err(|_| "stt path lock poisoned".to_string())?;
        let mut guard = self
            .engine
            .lock()
            .map_err(|_| "stt lock poisoned".to_string())?;

        if *active != path {
            *guard = None;
            *active = path;
        }

        if guard.is_none() {
            *guard = Some(WhisperEngine::load(&active)?);
        }
        Ok(())
    }

    pub fn transcribe(&self, samples: &[f32], settings: &SettingsState) -> Result<String, String> {
        let language = settings.get()?.whisper_language;
        self.ensure_loaded(settings)?;
        let guard = self
            .engine
            .lock()
            .map_err(|_| "stt lock poisoned".to_string())?;
        let engine = guard
            .as_ref()
            .ok_or_else(|| "whisper not loaded".to_string())?;
        engine.transcribe(samples, language)
    }
}
