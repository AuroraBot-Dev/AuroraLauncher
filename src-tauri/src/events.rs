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
    /// 瞬时下载速度（字节/秒）；只有下载阶段有值，供前端显示 “x MB/s”。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<u64>,
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

/// Bot 自身 stdout 的原始行（对话输出），供「对话」页展示。
pub fn bot_output(app: &AppHandle, line: impl Into<String>) {
    let _ = app.emit("bot-output", line.into());
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
            speed: None,
        },
    );
}

/// 下载专用进度：额外带上瞬时速度（字节/秒）。
pub fn progress_download(
    app: &AppHandle,
    kind: impl Into<String>,
    current: u64,
    total: Option<u64>,
    label: Option<String>,
    speed: u64,
) {
    let _ = app.emit(
        "progress",
        ProgressEvent {
            kind: kind.into(),
            current,
            total,
            label,
            speed: Some(speed),
        },
    );
}
