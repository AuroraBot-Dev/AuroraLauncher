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

use anyhow::Context;

fn main() {
    let runtime_paths = state::RuntimePaths::default_user().expect("解析运行时目录失败");
    let app_state = state::AppState::new(runtime_paths).expect("初始化 launcher 状态失败");

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::check_all_status,
            commands::install_all_deps,
            commands::install_dependency,
            commands::kernel_update,
            commands::start_bot,
            commands::stop_bot,
            commands::open_app_dir,
        ])
        .run(tauri::generate_context!())
        .with_context(|| "启动 AuroraLauncher 失败")
        .expect("Tauri 运行时退出异常");
}
