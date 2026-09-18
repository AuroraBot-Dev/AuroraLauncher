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
    Ok(AllStatus {
        dependency,
        kernel,
        process,
    })
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

/// 设置包下载源：`mirror`（国内 PyPI 镜像，默认）或 `official`。
#[tauri::command]
pub async fn set_download_source(source: String, state: State<'_, AppState>) -> Result<(), String> {
    let source = if source == "official" { "official" } else { "mirror" };
    state
        .settings
        .update(|settings| settings.download_source = source.to_string())
        .map_err(|error| format!("{error:#}"))
}

/// 设置 GitHub 加速前缀（如 `https://ghproxy.net/`）；留空表示直连 github.com。
/// 只影响工具安装包与 Python 解释器的下载，不影响 PyPI。
#[tauri::command]
pub async fn set_github_mirror(mirror: String, state: State<'_, AppState>) -> Result<(), String> {
    let mirror = mirror.trim().to_string();
    crate::manifest::validate_github_mirror(&mirror).map_err(|error| error.to_string())?;
    state
        .settings
        .update(|settings| settings.github_mirror = mirror)
        .map_err(|error| format!("{error:#}"))
}

/// 设置某工具的来源偏好：use_system=true 优先系统版本，false 优先启动器副本。
#[tauri::command]
pub async fn set_tool_source(
    kind: ToolKind,
    use_system: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .settings
        .update(|settings| {
            let tool = match kind {
                ToolKind::Python => &mut settings.python,
                ToolKind::Uv => &mut settings.uv,
                ToolKind::Git => &mut settings.git,
                ToolKind::Pnpm => &mut settings.pnpm,
            };
            tool.prefer_system = use_system;
        })
        .map_err(|error| format!("{error:#}"))
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
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AuroraProcessInfo, String> {
    let app_state = state.inner().clone();
    let service = BotService::new(&app_state);
    service.start(&app).await.map_err(|error| format!("{error:#}"))
}

/// 手动运行启动器 setup：初始化内核（工具/克隆/子模块/配置/venv/依赖）。
#[tauri::command]
pub async fn run_setup(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let app_state = state.inner().clone();
    let service = BotService::new(&app_state);
    if service.info().is_some() {
        return Err("AuroraBot 正在运行，请先停止后再初始化".into());
    }
    // setup 会安装工具、同步依赖，和「安装依赖」共用互斥，避免并发写同一工具目录
    acquire_install(&app_state)?;
    // 这里是用户明确点的「初始化」：如果 venv 的来源与当前选择不一致，就按当前来源重建
    let result = service
        .setup(&app, true)
        .await
        .map_err(|error| format!("{error:#}"));
    app_state.installing.store(false, Ordering::Release);
    result
}

/// 往 Bot 的 stdin 写入一行对话输入。
#[tauri::command]
pub fn send_bot_input(text: String, state: State<'_, AppState>) -> Result<(), String> {
    BotService::new(state.inner())
        .send_input(&text)
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

/// 在系统文件管理器中打开某工具的实际安装目录（自管优先，其次系统）。
#[tauri::command]
pub fn open_tool_dir(kind: ToolKind, state: State<'_, AppState>) -> Result<(), String> {
    let app_state = state.inner().clone();
    let path = ToolService::new(&app_state)
        .locate_dir(kind)
        .ok_or_else(|| format!("未找到可定位的 {kind} 安装目录"))?;
    open_path(&path).map_err(|error| error.to_string())
}

/// 在系统文件管理器中打开 AuroraBot 内核目录。
#[tauri::command]
pub fn open_kernel_dir(state: State<'_, AppState>) -> Result<(), String> {
    let path = &state.inner().paths.kernel_aurora;
    if !path.exists() {
        return Err("AuroraBot 内核尚未下载".to_string());
    }
    open_path(path).map_err(|error| error.to_string())
}

/// 调用内核的 `aurora about` 获取内核描述文本。
#[tauri::command]
pub async fn kernel_about(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let app_state = state.inner().clone();
    let service = KernelService::new(&app_state);
    let app_for_blocking = app.clone();
    tokio::task::spawn_blocking(move || service.about(&app_for_blocking))
        .await
        .map_err(|error| format!("获取内核描述任务失败：{error}"))?
        .map_err(|error| format!("{error:#}"))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub os: String,
    pub arch: String,
    pub root: String,
    pub download_source: String,
    pub github_mirror: String,
}

/// 关于页展示用的运行环境信息（系统、架构、运行时目录）。
#[tauri::command]
pub fn runtime_info(state: State<'_, AppState>) -> RuntimeInfo {
    RuntimeInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        root: state.inner().paths.root.display().to_string(),
        download_source: state.inner().settings.download_source(),
        github_mirror: state.inner().settings.github_mirror(),
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
