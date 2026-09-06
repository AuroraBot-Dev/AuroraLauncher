// 发布版以 GUI 子系统运行，避免启动时弹出控制台窗口（开发版保留控制台便于看日志）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod archive;
mod bot;
mod commands;
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
            commands::install_all_deps,
            commands::install_dependency,
            commands::kernel_update,
            commands::start_bot,
            commands::stop_bot,
            commands::open_app_dir,
            updater::check_launcher_update,
            updater::install_launcher_update,
        ])
        .build(tauri::generate_context!())
        .with_context(|| "启动 AuroraLauncher 失败")
        .expect("Tauri 运行时退出异常");

    app.run(|app_handle, event| {
        // 退出时把 launcher 启动的 Bot 一并停掉，避免留下孤儿 python 进程，
        // 否则残留进程会占用 tool 目录，导致后续“重装 python”删除时报 os error 5
        if let tauri::RunEvent::Exit = event {
            let app_state = app_handle.state::<state::AppState>();
            let _ = bot::BotService::new(&app_state).stop(app_handle);
        }
    });
}
