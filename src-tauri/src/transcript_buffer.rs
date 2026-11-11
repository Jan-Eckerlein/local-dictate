use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PasteScope {
    LastSegment,
    SinceLastCopy,
    All,
}

/// Session transcripts for multi-mode paste shortcuts.
#[derive(Clone, Default)]
pub struct TranscriptBuffer {
    segments: Arc<Mutex<BTreeMap<usize, String>>>,
    /// Inclusive upper bound of segment indices already covered by a "since last copy" paste.
    copied_through_index: Arc<Mutex<Option<usize>>>,
}

impl TranscriptBuffer {
    pub fn clear_session(&self) {
        if let Ok(mut guard) = self.segments.lock() {
            guard.clear();
        }
        if let Ok(mut guard) = self.copied_through_index.lock() {
            *guard = None;
        }
    }

    pub fn record_segment(&self, index: usize, text: String) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.segments.lock() {
            guard.insert(index, trimmed.to_string());
        }
    }

    pub fn latest(&self) -> Result<Option<String>, String> {
        let guard = self
            .segments
            .lock()
            .map_err(|_| "transcript buffer lock poisoned".to_string())?;
        Ok(guard.values().next_back().cloned())
    }

    pub fn text_for_paste(&self, scope: PasteScope) -> Result<String, String> {
        let guard = self
            .segments
            .lock()
            .map_err(|_| "transcript buffer lock poisoned".to_string())?;
        if guard.is_empty() {
            return Err("no transcript yet — speak while monitoring, then try again".into());
        }

        let text = match scope {
            PasteScope::LastSegment => guard
                .iter()
                .next_back()
                .map(|(_, t)| t.clone())
                .ok_or_else(|| "no transcript yet".to_string())?,
            PasteScope::SinceLastCopy => {
                let through = self
                    .copied_through_index
                    .lock()
                    .map_err(|_| "transcript buffer lock poisoned".to_string())?;
                let pending: Vec<&String> = guard
                    .iter()
                    .filter(|(index, _)| match *through {
                        None => true,
                        Some(last) => **index > last,
                    })
                    .map(|(_, t)| t)
                    .collect();
                if pending.is_empty() {
                    return Err("no new segments since last batch paste (⌘⇧B)".into());
                }
                join_lines(pending)
            }
            PasteScope::All => {
                let all: Vec<&String> = guard.values().collect();
                join_lines(all)
            }
        };
        Ok(text)
    }

    pub fn commit_copy(&self, scope: PasteScope) -> Result<(), String> {
        let guard = self
            .segments
            .lock()
            .map_err(|_| "transcript buffer lock poisoned".to_string())?;
        let mut through = self
            .copied_through_index
            .lock()
            .map_err(|_| "transcript buffer lock poisoned".to_string())?;

        match scope {
            PasteScope::LastSegment => {}
            PasteScope::SinceLastCopy => {
                let prev = *through;
                let max_included = guard
                    .keys()
                    .filter(|index| match prev {
                        None => true,
                        Some(last) => **index > last,
                    })
                    .max()
                    .copied();
                if let Some(max) = max_included {
                    *through = Some(max);
                }
            }
            PasteScope::All => {
                if let Some(max) = guard.keys().next_back() {
                    *through = Some(*max);
                }
            }
        }
        Ok(())
    }
}

fn join_lines(parts: Vec<&String>) -> String {
    parts
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
