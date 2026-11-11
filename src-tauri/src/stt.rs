use crate::segment::normalize_peak_for_stt;
use crate::settings::{SettingsState, WhisperLanguage};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub const DEFAULT_MODEL_FILE: &str = "ggml-base.bin";

/// `src-tauri/../models/ggml-base.bin` (repo root). Override with `LOCAL_DICTATE_WHISPER_MODEL`.
pub fn default_model_path() -> PathBuf {
    if let Ok(path) = std::env::var("LOCAL_DICTATE_WHISPER_MODEL") {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../models").join(DEFAULT_MODEL_FILE)
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperModelInfo {
    pub path: String,
    pub exists: bool,
    pub loaded: bool,
}

pub struct WhisperEngine {
    ctx: WhisperContext,
}

impl WhisperEngine {
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Err(format!(
                "model not found at {} — run scripts/download-whisper-model.sh",
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
    model_path: PathBuf,
    engine: Mutex<Option<WhisperEngine>>,
}

impl SttState {
    pub fn new() -> Self {
        Self {
            model_path: default_model_path(),
            engine: Mutex::new(None),
        }
    }

    pub fn model_info(&self) -> WhisperModelInfo {
        let loaded = self
            .engine
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false);
        WhisperModelInfo {
            path: self.model_path.display().to_string(),
            exists: self.model_path.exists(),
            loaded,
        }
    }

    fn ensure_loaded(&self) -> Result<(), String> {
        let mut guard = self
            .engine
            .lock()
            .map_err(|_| "stt lock poisoned".to_string())?;
        if guard.is_none() {
            *guard = Some(WhisperEngine::load(&self.model_path)?);
        }
        Ok(())
    }

    pub fn transcribe(&self, samples: &[f32], settings: &SettingsState) -> Result<String, String> {
        let language = settings.get()?.whisper_language;
        self.ensure_loaded()?;
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
