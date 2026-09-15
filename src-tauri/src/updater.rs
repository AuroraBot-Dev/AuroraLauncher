use std::sync::Mutex;

#[cfg(not(target_os = "windows"))]
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::events;

/// 缓存“检查时发现、待安装”的更新，避免再次请求远端。
pub struct PendingUpdate(pub Mutex<Option<Update>>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMeta {
    pub current_version: String,
    pub version: String,
    pub date: Option<String>,
    pub notes: Option<String>,
}

fn to_message(error: impl std::fmt::Display) -> String {
    format!("{error:#}")
}

#[tauri::command]
pub async fn check_launcher_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<Option<UpdateMeta>, String> {
    let update = app
        .updater()
        .map_err(to_message)?
        .check()
        .await
        .map_err(to_message)?;

    let meta = update.as_ref().map(|item| UpdateMeta {
        current_version: item.current_version.clone(),
        version: item.version.clone(),
        date: item.date.map(|date| {
            chrono::DateTime::from_timestamp(date.unix_timestamp(), 0)
                .map(|time| time.to_rfc3339())
                .unwrap_or_default()
        }),
        notes: item.body.clone(),
    });

    *pending
        .0
        .lock()
        .map_err(|_| "更新状态锁已损坏".to_string())? = update;

    Ok(meta)
}

#[tauri::command]
pub async fn install_launcher_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<(), String> {
    // 取出缓存的更新并把锁的占用范围收紧到该语句块内，避免非 Send 的
    // MutexGuard 在后面的 await 中存活。
    let update = {
        let mut guard = pending
            .0
            .lock()
            .map_err(|_| "更新状态锁已损坏".to_string())?;
        guard
            .take()
            .ok_or_else(|| "当前没有已检查到的更新，请先检查更新".to_string())?
    };

    let app_for_events = app.clone();
    events::log(&app_for_events, "info", "开始下载并安装 AuroraLauncher 更新…");

    let mut downloaded: u64 = 0;
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                // kind 用 "launcher" 与内核的 "update" 区分开：更新弹窗有自己的进度条，
                // 不会让「内核」页或主页的进度条跟着动
                events::progress(
                    &app_for_events,
                    "launcher",
                    downloaded,
                    total,
                    Some("正在下载更新".to_string()),
                );
            },
            || {
                events::progress(&app_for_events, "launcher", 0, None, None);
            },
        )
        .await
        .map_err(|error| format!("安装更新失败：{error:#}"))?;

    events::log(&app_for_events, "success", "更新已安装，正在重启应用…");

    // Windows 的安装器会自行退出并重新拉起应用；macOS/Linux 需手动重启。
    #[cfg(not(target_os = "windows"))]
    {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(600)).await;
            handle.restart();
        });
    }

    Ok(())
}
