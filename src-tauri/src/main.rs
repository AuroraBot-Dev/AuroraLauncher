// 发布版以 GUI 子系统运行，避免启动时弹出控制台窗口（开发版保留控制台便于看日志）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod archive;
mod bot;
mod commands;
mod config;
mod download;
mod events;
mod hash;
mod kernel;
mod manifest;
mod platform;
mod sandbox;
mod state;
mod tools;
mod updater;

use anyhow::Context;
use tauri::Manager;

fn main() {
    let runtime_paths = state::RuntimePaths::default_user().expect("解析运行时目录失败");
    let app_state = state::AppState::new(runtime_paths).expect("初始化 launcher 状态失败");

    let app = tauri::Builder::default()
        .manage(app_state)
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updater::PendingUpdate(std::sync::Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            commands::check_all_status,
            commands::install_dependency,
            commands::set_tool_dir,
            commands::set_tool_source,
            commands::set_download_source,
            commands::set_github_mirror,
            commands::set_user_zoom,
            commands::kernel_update,
            commands::run_setup,
            commands::start_bot,
            commands::send_bot_input,
            commands::stop_bot,
            commands::open_app_dir,
            commands::open_tool_dir,
            commands::open_kernel_dir,
            commands::kernel_about,
            config::read_launcher_config,
            config::set_env_value,
            config::set_app_enabled,
            commands::open_external_url,
            commands::runtime_info,
            updater::check_launcher_update,
            updater::install_launcher_update,
        ])
        .build(tauri::generate_context!())
        .with_context(|| "启动 AuroraLauncher 失败")
        .expect("Tauri 运行时退出异常");

    app.run(|app_handle, event| match event {
        // 退出时把 launcher 启动的 Bot 一并停掉，避免留下孤儿 python 进程，
        // 否则残留进程会占用 tool 目录，导致后续“重装 python”删除时报 os error 5
        tauri::RunEvent::Exit => {
            let app_state = app_handle.state::<state::AppState>();
            let _ = bot::BotService::new(&app_state).stop(app_handle);
        }
        // 窗口变大时按比例放大整个界面（等价浏览器缩放）：否则只是留白变多、字还是那么小
        tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::Resized(_),
            ..
        } => sync_zoom(app_handle, &label),
        _ => {}
    });
}

/// 设计基准宽度，与 tauri.conf.json 里窗口的初始宽度一致。
const DESIGN_WIDTH: f64 = 960.0;
/// 放大增益：窗口变宽时只按此比例放大（做阻尼），否则字会跟着窗口一起变得太大。
const ZOOM_GAIN: f64 = 0.5;
/// 自动放大上限：窗口再宽也不无限放大，避免在 4K 屏上字大得离谱。
const MAX_AUTO_ZOOM: f64 = 1.35;
/// 用户手动缩放的上下限（Ctrl+滚轮 / Ctrl+加减）。
pub(crate) const MIN_USER_ZOOM: f64 = 0.6;
pub(crate) const MAX_USER_ZOOM: f64 = 2.0;

/// 按窗口逻辑宽度相对设计宽度的比例设置 WebView 缩放因子（只放大、不缩小），
/// 再叠加用户手动缩放（`settings.user_zoom`）。用 WebView 缩放而不是 CSS `zoom`：
/// 前者会改变 CSS 视口尺寸，`100vh` 之类仍然正确。
pub(crate) fn sync_zoom(app: &tauri::AppHandle, label: &str) {
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    let Ok(scale_factor) = window.scale_factor() else {
        return;
    };
    let Ok(size) = window.inner_size() else {
        return;
    };
    let logical_width = f64::from(size.width) / scale_factor;
    // 阻尼自动放大：例如窗口宽到 1440 时只放大到 1.25 而不是 1.5
    let ratio = logical_width / DESIGN_WIDTH;
    let base = (1.0 + (ratio - 1.0) * ZOOM_GAIN).clamp(1.0, MAX_AUTO_ZOOM);
    // 叠加用户手动缩放，并夹在允许范围内
    let user = app
        .state::<state::AppState>()
        .settings
        .snapshot()
        .user_zoom
        .clamp(MIN_USER_ZOOM, MAX_USER_ZOOM);
    let zoom = (base * user).clamp(MIN_USER_ZOOM, MAX_AUTO_ZOOM * MAX_USER_ZOOM);
    let _ = window.set_zoom(zoom);
}
