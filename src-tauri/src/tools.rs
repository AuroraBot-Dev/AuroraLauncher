use std::path::{Path, PathBuf};
use std::process::Output;

use anyhow::{anyhow, bail, Context, Result};
use chrono::Local;
use tauri::AppHandle;

use crate::archive::extract;
use crate::download::download_verified;
use crate::events;
use crate::manifest::{self, ToolKind};
use crate::platform::is_windows;
use crate::sandbox::Sandbox;
use crate::state::{
    AppState, BotRegistry, DependencyStatus, RuntimePaths, Settings, SettingsStore, ToolMeta,
    ToolState,
};

#[derive(Clone)]
pub struct ToolService {
    pub paths: std::sync::Arc<RuntimePaths>,
    pub settings: std::sync::Arc<SettingsStore>,
    pub bot: std::sync::Arc<BotRegistry>,
}

impl ToolService {
    pub fn new(state: &AppState) -> Self {
        Self {
            paths: state.paths.clone(),
            settings: state.settings.clone(),
            bot: state.bot.clone(),
        }
    }

    pub fn is_installed_without_network(&self, kind: ToolKind) -> bool {
        if self.bot.info().is_some() {
            return true;
        }
        let state = self.setting_state(kind);
        if state.version.is_empty() {
            return false;
        }
        let exe = self.exe_path(kind);
        exe.exists()
    }

    pub async fn is_installed(&self, kind: ToolKind) -> bool {
        if kind == ToolKind::Python {
            return self.python_is_installed().await;
        }
        self.is_installed_without_network(kind)
    }

    pub async fn install(&self, kind: ToolKind, app: &AppHandle) -> Result<()> {
        if self.bot.info().is_some() {
            bail!("AuroraBot 正在运行，请先停止后再修改运行工具");
        }
        if self.is_installed(kind).await {
            events::log(app, "success", format!("{} 已就绪", kind));
            return Ok(());
        }
        events::log(app, "info", format!("开始安装 {}", kind));
        events::progress(app, kind.label(), 0, None, Some(format!("下载 {}", kind)));

        match kind {
            ToolKind::Python => self.install_python(app).await?,
            ToolKind::Uv => self.install_uv(app).await?,
            ToolKind::Git | ToolKind::Pnpm => {
                let requirement = manifest::requirement(kind)?;
                let package = requirement
                    .package
                    .ok_or_else(|| anyhow!("工具 {} 缺少包描述", kind))?;
                self.install_package(kind, &package, app).await?;
            }
        }
        events::log(app, "success", format!("{} 安装完成", kind));
        events::progress(app, kind.label(), 0, None, None);
        Ok(())
    }

    pub async fn install_all(&self, app: &AppHandle) -> Result<()> {
        for kind in [
            ToolKind::Git,
            ToolKind::Uv,
            ToolKind::Python,
            ToolKind::Pnpm,
        ] {
            self.install(kind, app).await?;
        }
        Ok(())
    }

    async fn install_python(&self, app: &AppHandle) -> Result<()> {
        if !self.uv_exe().exists() {
            self.install_uv(app).await?;
        }
        let requirement = manifest::requirement(ToolKind::Python)?;
        let mut cmd = Sandbox::new(&self.paths).command(&self.uv_exe());
        cmd.arg("python")
            .arg("install")
            .arg("--install-dir")
            .arg(&self.paths.tools_python)
            .arg(&requirement.version);
        let output = capture_output(cmd, "uv python install").await?;
        if !output.status.success() {
            bail!(
                "Python 安装失败：{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let now = Local::now().to_rfc3339();
        self.settings.update(|settings| {
            set_tool_state(
                settings,
                ToolKind::Python,
                ToolState {
                    version: requirement.version,
                    sha256: String::new(),
                    url: "uv-managed-python-build-standalone".into(),
                    installed_at: Some(now),
                },
            );
        })?;
        Ok(())
    }

    async fn install_uv(&self, app: &AppHandle) -> Result<()> {
        if self.is_installed(ToolKind::Uv).await {
            return Ok(());
        }
        let requirement = manifest::requirement(ToolKind::Uv)?;
        let package = requirement
            .package
            .ok_or_else(|| anyhow!("uv 缺少包描述"))?;
        self.install_package(ToolKind::Uv, &package, app).await
    }

    async fn install_package(
        &self,
        kind: ToolKind,
        package: &manifest::PackageSpec,
        app: &AppHandle,
    ) -> Result<()> {
        let target = self.ensure_tool_dir(kind);
        let archive = self.paths.downloads.join(&package.archive_name);
        if !archive.exists() {
            let app_for_progress = app.clone();
            let kind_label = kind.label().to_string();
            download_verified(
                &package.url,
                &archive,
                package.sha256.as_deref(),
                move |current, total| {
                    events::progress(
                        &app_for_progress,
                        kind_label.clone(),
                        current,
                        (total > 0).then_some(total),
                        Some(format!("下载 {}", kind_label)),
                    );
                },
            )
            .await?;
        }

        let versioned = format!("{}-{}", kind, package.version);
        let staging = self.paths.staging.join(versioned);
        clean_dir(&staging)?;
        std::fs::create_dir_all(&staging)?;

        match kind {
            ToolKind::Git => {
                extract(archive.clone(), staging.clone()).await?;
                let exe = if is_windows() {
                    staging.join("cmd").join("git.exe")
                } else {
                    staging.join("bin").join("git")
                };
                if !exe.exists() {
                    bail!("Git 解压后未找到 {}", exe.display());
                }
            }
            ToolKind::Uv => {
                let payload = staging.join("payload");
                extract(archive.clone(), payload.clone()).await?;
                let found = find_file_recursive(&payload, &self.exe_name(kind))?;
                let destination = staging.join(self.exe_name(kind));
                std::fs::copy(&found, &destination)?;
                make_executable(&destination)?;
            }
            ToolKind::Pnpm => {
                let destination = staging.join(self.exe_name(kind));
                std::fs::copy(&archive, &destination)?;
                make_executable(&destination)?;
            }
            ToolKind::Python => unreachable!("Python 由 uv python install 管理"),
        }

        let backup = self.paths.staging.join(format!("{kind}-backup"));
        clean_dir(&backup)?;
        if target.exists() {
            std::fs::rename(&target, &backup)?;
        }
        std::fs::rename(&staging, &target)?;
        let _ = std::fs::remove_dir_all(&backup);

        let now = Local::now().to_rfc3339();
        self.settings.update(|settings| {
            set_tool_state(
                settings,
                kind,
                ToolState {
                    version: package.version.clone(),
                    sha256: package.sha256.clone().unwrap_or_default(),
                    url: package.url.clone(),
                    installed_at: Some(now),
                },
            );
        })?;

        let mut verify = Sandbox::new(&self.paths).command(&self.exe_path(kind));
        verify.arg("--version");
        let output = capture_output(verify, "工具自检").await?;
        if !output.status.success() {
            bail!(
                "{} 自检失败：{}",
                kind,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    async fn python_is_installed(&self) -> bool {
        if !self.uv_exe().exists() {
            return false;
        }
        let requirement = manifest::requirement(ToolKind::Python).unwrap();
        let mut cmd = Sandbox::new(&self.paths).command(&self.uv_exe());
        cmd.arg("python")
            .arg("find")
            .arg("--no-project")
            .arg("--python-preference")
            .arg("only-managed")
            .arg(requirement.version);
        let output = capture_output(cmd, "uv python find").await;
        match output {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    pub fn setting_state(&self, kind: ToolKind) -> ToolState {
        let settings = self.settings.snapshot();
        match kind {
            ToolKind::Python => settings.python,
            ToolKind::Uv => settings.uv,
            ToolKind::Git => settings.git,
            ToolKind::Pnpm => settings.pnpm,
        }
    }

    fn ensure_tool_dir(&self, kind: ToolKind) -> PathBuf {
        match kind {
            ToolKind::Python => self.paths.tools_python.clone(),
            ToolKind::Uv => self.paths.tools_uv.clone(),
            ToolKind::Git => self.paths.tools_git.clone(),
            ToolKind::Pnpm => self.paths.tools_pnpm.clone(),
        }
    }

    pub fn exe_path(&self, kind: ToolKind) -> PathBuf {
        match kind {
            ToolKind::Python => self.paths.tools_python.clone(),
            ToolKind::Uv => self.paths.uv_exe(),
            ToolKind::Git => self.paths.git_exe(),
            ToolKind::Pnpm => self.paths.pnpm_exe(),
        }
    }

    fn exe_name(&self, kind: ToolKind) -> String {
        match kind {
            ToolKind::Uv => format!("uv{}", if is_windows() { ".exe" } else { "" }),
            ToolKind::Pnpm => format!("pnpm{}", if is_windows() { ".exe" } else { "" }),
            ToolKind::Git | ToolKind::Python => String::new(),
        }
    }

    fn uv_exe(&self) -> PathBuf {
        self.paths.uv_exe()
    }
}

fn set_tool_state(settings: &mut Settings, kind: ToolKind, state: ToolState) {
    match kind {
        ToolKind::Python => settings.python = state,
        ToolKind::Uv => settings.uv = state,
        ToolKind::Git => settings.git = state,
        ToolKind::Pnpm => settings.pnpm = state,
    }
}

pub async fn dependency_status(service: &ToolService) -> DependencyStatus {
    let settings = service.settings.snapshot();
    let uv_ready = service.is_installed(ToolKind::Uv).await;
    let python_ready = service.is_installed(ToolKind::Python).await;
    let git_ready = service.is_installed(ToolKind::Git).await;
    let pnpm_ready = service.is_installed(ToolKind::Pnpm).await;
    DependencyStatus {
        python: ToolMeta {
            version: settings.python.version,
            installed: python_ready,
            installed_at: settings.python.installed_at,
        },
        uv: ToolMeta {
            version: settings.uv.version,
            installed: uv_ready,
            installed_at: settings.uv.installed_at,
        },
        git: ToolMeta {
            version: settings.git.version,
            installed: git_ready,
            installed_at: settings.git.installed_at,
        },
        pnpm: ToolMeta {
            version: settings.pnpm.version,
            installed: pnpm_ready,
            installed_at: settings.pnpm.installed_at,
        },
    }
}

fn clean_dir(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn find_file_recursive(root: &Path, name: &str) -> Result<PathBuf> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Ok(found) = find_file_recursive(&path, name) {
                return Ok(found);
            }
        } else if path.file_name().and_then(|name| name.to_str()) == Some(name) {
            return Ok(path);
        }
    }
    Err(anyhow!("在 {} 中未找到 {}", root.display(), name))
}

async fn capture_output(mut cmd: std::process::Command, label: &'static str) -> Result<Output> {
    tokio::task::spawn_blocking(move || cmd.output().with_context(|| format!("执行 {label} 失败")))
        .await
        .with_context(|| format!("{label} 任务失败"))?
}
