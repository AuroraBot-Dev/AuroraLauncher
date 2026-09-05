use serde::Serialize;
use tauri::{AppHandle, State};

use crate::bot::BotService;
use crate::kernel::KernelService;
use crate::manifest::ToolKind;
use crate::state::{AppState, AuroraProcessInfo, DependencyStatus, KernelStatus};
use crate::tools::{dependency_status, ToolService};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllStatus {
    pub dependency: DependencyStatus,
    pub kernel: KernelStatus,
    pub process: Option<AuroraProcessInfo>,
}

#[tauri::command]
pub async fn check_all_status(state: State<'_, AppState>) -> Result<AllStatus, String> {
    let app_state = state.inner().clone();
    let tools = ToolService::new(&app_state);
    let dependency = dependency_status(&tools).await;
    let kernel = KernelService::new(&app_state)
        .status()
        .map_err(|error| error.to_string())?;
    let process = BotService::new(&app_state).info();
    let all = AllStatus {
        dependency,
        kernel,
        process,
    };
    // 诊断：把每次探测结果追加到 <tool>/logs/launcher-status.log，便于排查“显示缺失”
    if let Ok(json) = serde_json::to_string(&all) {
        let file = app_state.paths.logs.join("launcher-status.log");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .and_then(|mut handle| {
                use std::io::Write;
                handle.write_all(format!("{json}\n").as_bytes())
            });
    }
    Ok(all)
}

#[tauri::command]
pub async fn install_all_deps(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let app_state = state.inner().clone();
    let service = ToolService::new(&app_state);
    service
        .install_all(&app)
        .await
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn install_dependency(
    kind: ToolKind,
    force: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let app_state = state.inner().clone();
    let service = ToolService::new(&app_state);
    service
        .install(kind, &app, force.unwrap_or(false))
        .await
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn kernel_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<KernelStatus, String> {
    let app_state = state.inner().clone();
    let service = KernelService::new(&app_state);
    let app_for_blocking = app.clone();
    tokio::task::spawn_blocking(move || service.update(&app_for_blocking))
        .await
        .map_err(|error| format!("内核更新任务失败：{error}"))?
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn start_bot(
    headless: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AuroraProcessInfo, String> {
    let app_state = state.inner().clone();
    let service = BotService::new(&app_state);
    service
        .start(headless, &app)
        .await
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub fn stop_bot(app: AppHandle, state: State<'_, AppState>) -> Result<AuroraProcessInfo, String> {
    BotService::new(state.inner())
        .stop(&app)
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub fn open_app_dir(state: State<'_, AppState>) -> Result<(), String> {
    open_path(&state.inner().paths.root).map_err(|error| error.to_string())
}

fn open_path(path: &std::path::Path) -> anyhow::Result<()> {
    let path_str = path.to_string_lossy().to_string();
    if cfg!(target_os = "windows") {
        std::process::Command::new("explorer")
            .arg(&path_str)
            .spawn()?;
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(&path_str).spawn()?;
    } else {
        std::process::Command::new("xdg-open")
            .arg(&path_str)
            .spawn()?;
    }
    Ok(())
}
