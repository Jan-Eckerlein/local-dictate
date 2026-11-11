use serde::{Deserialize, Serialize};

/// Severity chosen where the notice is created, not inferred from the message.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NoticeKind {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NoticeAction {
    Accessibility,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppNotice {
    pub message: String,
    pub kind: NoticeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<NoticeAction>,
}

impl AppNotice {
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message, NoticeKind::Info)
    }

    pub fn warn(message: impl Into<String>) -> Self {
        Self::new(message, NoticeKind::Warn)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(message, NoticeKind::Error)
    }

    pub fn with_action(mut self, action: NoticeAction) -> Self {
        self.action = Some(action);
        self
    }

    fn new(message: impl Into<String>, kind: NoticeKind) -> Self {
        Self {
            message: message.into(),
            kind,
            action: None,
        }
    }
}
