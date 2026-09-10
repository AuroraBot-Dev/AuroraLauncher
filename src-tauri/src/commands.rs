use serde::Serialize;
use std::sync::atomic::Ordering;
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
    acquire_install(&app_state)?;
    let service = ToolService::new(&app_state);
    let result = service
        .install_all(&app)
        .await
        .map_err(|error| format!("{error:#}"));
    app_state.installing.store(false, Ordering::Release);
    result
}

#[tauri::command]
pub async fn install_dependency(
    kind: ToolKind,
    force: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let app_state = state.inner().clone();
    acquire_install(&app_state)?;
    let service = ToolService::new(&app_state);
    let result = service
        .install(kind, &app, force.unwrap_or(false))
        .await
        .map_err(|error| format!("{error:#}"));
    app_state.installing.store(false, Ordering::Release);
    result
}

/// 安装互斥：已有安装任务进行中时直接拒绝，保证一次只装一个。
fn acquire_install(state: &AppState) -> Result<(), String> {
    state
        .installing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map(|_| ())
        .map_err(|_| "已有安装任务进行中，请等待当前任务完成".to_string())
}

/// 设置某工具的自定义安装目录；传空字符串恢复默认目录。
#[tauri::command]
pub async fn set_tool_dir(
    kind: ToolKind,
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let app_state = state.inner().clone();
    app_state
        .settings
        .update(|settings| {
            settings
                .tool_dirs
                .set(kind, path.unwrap_or_default().trim().to_string());
        })
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub os: String,
    pub arch: String,
    pub root: String,
}

/// 关于页展示用的运行环境信息（系统、架构、运行时目录）。
#[tauri::command]
pub fn runtime_info(state: State<'_, AppState>) -> RuntimeInfo {
    RuntimeInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        root: state.inner().paths.root.display().to_string(),
    }
}

/// 用系统默认浏览器打开外部链接；仅允许 http/https，避免被当作本地命令执行。
#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    let url = url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("仅支持打开 http/https 链接".to_string());
    }
    open_url(url).map_err(|error| error.to_string())
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

fn open_url(url: &str) -> anyhow::Result<()> {
    if cfg!(target_os = "windows") {
        std::process::Command::new("explorer").arg(url).spawn()?;
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()?;
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}
