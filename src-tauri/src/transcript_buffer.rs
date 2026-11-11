use std::sync::{Arc, Mutex};

/// Latest successful STT text (for global paste shortcut).
#[derive(Clone, Default)]
pub struct TranscriptBuffer {
    inner: Arc<Mutex<Option<String>>>,
}

impl TranscriptBuffer {
    pub fn set_latest(&self, text: String) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.inner.lock() {
            *guard = Some(trimmed.to_string());
        }
    }

    pub fn latest(&self) -> Result<Option<String>, String> {
        self.inner
            .lock()
            .map(|g| g.clone())
            .map_err(|_| "transcript buffer lock poisoned".to_string())
    }

    pub fn take_for_paste(&self) -> Result<String, String> {
        let text = self
            .latest()?
            .filter(|t| !t.is_empty())
            .ok_or_else(|| "no transcript yet — speak while monitoring, then try again".to_string())?;
        Ok(text)
    }
}
