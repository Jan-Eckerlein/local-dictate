use crate::notice::AppNotice;
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
    /// Inclusive upper bound already covered by any explicit paste (⌘⇧V, ⌘⇧B, or ⌘⇧A).
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

    pub fn text_for_paste(&self, scope: PasteScope) -> Result<String, AppNotice> {
        let guard = self
            .segments
            .lock()
            .map_err(|_| AppNotice::error("transcript buffer lock poisoned"))?;
        if guard.is_empty() {
            return Err(AppNotice::warn(
                "no transcript yet — speak while monitoring, then try again",
            ));
        }

        let text = match scope {
            PasteScope::LastSegment => guard
                .iter()
                .next_back()
                .map(|(_, t)| t.clone())
                .ok_or_else(|| AppNotice::warn("no transcript yet"))?,
            PasteScope::SinceLastCopy => {
                let through = self
                    .copied_through_index
                    .lock()
                    .map_err(|_| AppNotice::error("transcript buffer lock poisoned"))?;
                let pending: Vec<&String> = guard
                    .iter()
                    .filter(|(index, _)| match *through {
                        None => true,
                        Some(last) => **index > last,
                    })
                    .map(|(_, t)| t)
                    .collect();
                if pending.is_empty() {
                    return Err(AppNotice::info(
                        "no new segments since last paste (⌘⇧V, ⌘⇧B, or ⌘⇧A)",
                    ));
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

        let advanced = match scope {
            // Last and all both end on the newest segment, so the next batch starts after it.
            PasteScope::LastSegment | PasteScope::All => guard.keys().next_back().copied(),
            PasteScope::SinceLastCopy => {
                let prev = *through;
                guard
                    .keys()
                    .filter(|index| match prev {
                        None => true,
                        Some(last) => **index > last,
                    })
                    .max()
                    .copied()
            }
        };
        if let Some(max) = advanced {
            *through = Some(match *through {
                Some(prev) => prev.max(max),
                None => max,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer_with(texts: &[&str]) -> TranscriptBuffer {
        let buf = TranscriptBuffer::default();
        for (index, text) in texts.iter().enumerate() {
            buf.record_segment(index, (*text).to_string());
        }
        buf
    }

    #[test]
    fn last_paste_moves_batch_start() {
        let buf = buffer_with(&["a", "b"]);
        buf.commit_copy(PasteScope::SinceLastCopy).unwrap();

        buf.record_segment(2, "c".into());
        buf.record_segment(3, "d".into());
        buf.commit_copy(PasteScope::LastSegment).unwrap();

        buf.record_segment(4, "e".into());
        assert_eq!(
            buf.text_for_paste(PasteScope::SinceLastCopy).unwrap(),
            "e"
        );
    }

    #[test]
    fn all_paste_moves_batch_start() {
        let buf = buffer_with(&["a", "b", "c"]);
        buf.commit_copy(PasteScope::All).unwrap();
        assert!(buf.text_for_paste(PasteScope::SinceLastCopy).is_err());

        buf.record_segment(3, "d".into());
        assert_eq!(
            buf.text_for_paste(PasteScope::SinceLastCopy).unwrap(),
            "d"
        );
    }
}

fn join_lines(parts: Vec<&String>) -> String {
    parts
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
