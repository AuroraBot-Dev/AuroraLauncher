use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub kind: String,
    pub current: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEvent {
    pub level: String,
    pub message: String,
}

pub fn log(app: &AppHandle, level: impl Into<String>, message: impl Into<String>) {
    let _ = app.emit(
        "log",
        LogEvent {
            level: level.into(),
            message: message.into(),
        },
    );
}

pub fn progress(
    app: &AppHandle,
    kind: impl Into<String>,
    current: u64,
    total: Option<u64>,
    label: Option<String>,
) {
    let _ = app.emit(
        "progress",
        ProgressEvent {
            kind: kind.into(),
            current,
            total,
            label,
        },
    );
}
