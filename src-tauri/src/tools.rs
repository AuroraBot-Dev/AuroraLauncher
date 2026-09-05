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

    /// 受管安装目录里是否已存在该工具（python 由 uv 管理，无单一 exe）
    pub fn managed_exe(&self, kind: ToolKind) -> Option<PathBuf> {
        match kind {
            ToolKind::Python => None,
            _ => {
                let exe = self.exe_path(kind);
                exe.is_file().then_some(exe)
            }
        }
    }

    /// 实际使用的工具路径：优先 launcher 自管的，其次系统 PATH 上可用的
    pub fn resolve_exe(&self, kind: ToolKind) -> Option<PathBuf> {
        match kind {
            ToolKind::Python => self.system_python(),
            _ => self.managed_exe(kind).or_else(|| system_exe_path(kind)),
        }
    }

    pub fn uv_command_exe(&self) -> Option<PathBuf> {
        self.resolve_exe(ToolKind::Uv)
    }

    /// 系统 PATH 上的 Python（自管 Python 缺失时兜底）
    pub fn system_python(&self) -> Option<PathBuf> {
        system_python_info().map(|(path, _)| path)
    }

    /// 该工具是否可用：launcher 自管或系统已装均可
    pub async fn is_installed(&self, kind: ToolKind) -> bool {
        match kind {
            ToolKind::Python => self.python_ready().await,
            _ => match self.managed_exe(kind) {
                Some(_) => true,
                None => system_exe_path(kind)
                    .and_then(|exe| probe_version(&exe))
                    .is_some(),
            },
        }
    }

    pub async fn install(&self, kind: ToolKind, app: &AppHandle, force: bool) -> Result<()> {
        if self.bot.info().is_some() {
            bail!("AuroraBot 正在运行，请先停止后再修改运行工具");
        }
        if force {
            // 真·重装：清掉受管副本与记录状态后重走安装。同时丢弃已下载的安装包
            // 缓存，让“重装”重新下载 → 前端能显示真实百分比进度条
            events::log(app, "info", format!("重新安装 {}", kind));
            let managed = self.ensure_tool_dir(kind);
            clean_dir(&managed).with_context(|| {
                format!(
                    "无法清理 {} 的受管目录，可能是仍有旧进程占用：请先在底部按钮停止 Bot 后重试",
                    kind
                )
            })?;
            std::fs::create_dir_all(&managed)?;
            self.settings.update(|settings| {
                set_tool_state(settings, kind, ToolState::default());
            })?;
            if !matches!(kind, ToolKind::Python) {
                if let Ok(requirement) = manifest::requirement(kind) {
                    if let Some(package) = requirement.package {
                        let _ = std::fs::remove_file(self.paths.downloads.join(&package.archive_name));
                    }
                }
            }
        } else if self.is_installed(kind).await {
            events::log(app, "success", format!("{} 已就绪", kind));
            return Ok(());
        }
        events::log(app, "info", format!("开始安装 {}", kind));
        events::progress(app, kind.label(), 0, None, Some(format!("下载 {}", kind)));

        // 无论成功失败都把进度态清掉，避免前端残留“下载中/转圈”
        let result = self.run_install(kind, app).await;
        if result.is_err() {
            events::progress(app, kind.label(), 0, None, None);
        }
        result?;

        events::log(app, "success", format!("{} 安装完成", kind));
        events::progress(app, kind.label(), 0, None, None);
        Ok(())
    }

    async fn run_install(&self, kind: ToolKind, app: &AppHandle) -> Result<()> {
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
        Ok(())
    }

    pub async fn install_all(&self, app: &AppHandle) -> Result<()> {
        for kind in [
            ToolKind::Git,
            ToolKind::Uv,
            ToolKind::Python,
            ToolKind::Pnpm,
        ] {
            self.install(kind, app, false).await?;
        }
        Ok(())
    }

    async fn install_python(&self, app: &AppHandle) -> Result<()> {
        if self.uv_command_exe().is_none() {
            self.install_uv(app).await?;
        }
        let uv = self
            .uv_command_exe()
            .ok_or_else(|| anyhow!("无法定位可用的 uv，无法安装 Python"))?;
        let requirement = manifest::requirement(ToolKind::Python)?;
        let mut cmd = Sandbox::new(&self.paths).command(&uv);
        cmd.arg("python")
            .arg("install")
            .arg("--install-dir")
            .arg(&self.paths.tools_python)
            .arg(&requirement.version);
        // uv 下载/解压解释器耗时较长且没有字节级进度，用不定进度提示避免“看起来卡住”
        events::progress(
            app,
            "python",
            0,
            None,
            Some(format!("正在通过 uv 安装 Python {}（下载解释器中…）", requirement.version)),
        );
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
        // 注意：不要在这里自检 is_installed —— 调用方（install/bot 等）已先判断过，
        // 且“重装”时受管目录已被清空、系统里却可能有 uv，自检会导致受管副本永远装不上。
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
        // 解压/落盘阶段没有字节级进度，改提示文案避免前端停留在“下载 xx%”后像卡死
        events::progress(
            app,
            kind.label(),
            0,
            None,
            Some(format!("正在安装 {}", kind.label())),
        );

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

    /// 仅 launcher 用 uv 自管的 Python 是否就绪（不含系统 Python）
    pub async fn managed_python_ready(&self) -> bool {
        let Some(uv) = self.uv_command_exe() else {
            return false;
        };
        let requirement = manifest::requirement(ToolKind::Python).unwrap();
        let mut cmd = Sandbox::new(&self.paths).command(&uv);
        cmd.arg("python")
            .arg("find")
            .arg("--no-project")
            .arg("--python-preference")
            .arg("only-managed")
            .arg(requirement.version);
        match capture_output(cmd, "uv python find").await {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    /// Python 是否可用：自管或系统已装均可
    async fn python_ready(&self) -> bool {
        if self.managed_python_ready().await {
            return true;
        }
        system_python_info().is_some()
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
}

fn set_tool_state(settings: &mut Settings, kind: ToolKind, state: ToolState) {
    match kind {
        ToolKind::Python => settings.python = state,
        ToolKind::Uv => settings.uv = state,
        ToolKind::Git => settings.git = state,
        ToolKind::Pnpm => settings.pnpm = state,
    }
}

/// 在系统 PATH 中查找可执行文件。
/// Windows：必须带 PATHEXT 扩展名才算可执行（目录里可能有无用的裸名 bash shim）；
/// 未命中时再补查 npm / uv 常见安装目录，兼容工具不在 GUI 会话 PATH 里的情况。
fn which_in_path(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    if is_windows() {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(appdata).join("npm"));
        }
        if let Some(home) = std::env::var_os("USERPROFILE") {
            dirs.push(PathBuf::from(home).join(".local").join("bin"));
        }
    }

    for dir in dirs {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let base = dir.join(name);
        if !is_windows() {
            if base.is_file() {
                return Some(base);
            }
            continue;
        }
        let pathext = std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
        for ext in pathext.split(';') {
            let ext = ext.trim();
            if ext.is_empty() {
                continue;
            }
            let candidate = PathBuf::from(format!("{}{}", base.display(), ext));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 运行 `<exe> --version` 并返回首行输出。
/// `.cmd/.bat` 批处理包装（如 npm 全局 pnpm.cmd）不实际执行：
/// 它们经 cmd 启动后还会再拉起 node 等控制台子进程，逐个弹终端窗，
/// 这里只按文件存在性识别，避免启动探测时闪窗。
fn probe_version(exe: &Path) -> Option<String> {
    let ext = exe
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if ext == "cmd" || ext == "bat" {
        return exe
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned());
    }
    // Windows 下父进程是 GUI：控制台子进程会各自新开终端窗，需隐藏（CREATE_NO_WINDOW）
    let mut command = std::process::Command::new(exe);
    command.arg("--version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let bytes = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let line = String::from_utf8_lossy(bytes)
        .lines()
        .next()?
        .trim()
        .to_string();
    if line.is_empty() {
        None
    } else {
        Some(line)
    }
}

/// 系统 PATH 上是否有该工具
pub fn system_exe_path(kind: ToolKind) -> Option<PathBuf> {
    let stems: &[&str] = match kind {
        ToolKind::Git => &["git"],
        ToolKind::Uv => &["uv"],
        ToolKind::Pnpm => &["pnpm"],
        ToolKind::Python => {
            if is_windows() {
                &["python", "python3"]
            } else {
                &["python3", "python"]
            }
        }
    };
    stems.iter().find_map(|name| which_in_path(name))
}

/// 在系统 PATH 上找一个可用并能跑通的 Python，返回 (路径, 版本)
fn system_python_info() -> Option<(PathBuf, String)> {
    let stems: &[&str] = if is_windows() {
        &["python", "python3"]
    } else {
        &["python3", "python"]
    };
    for name in stems {
        if let Some(exe) = which_in_path(name) {
            if let Some(version) = probe_version(&exe) {
                return Some((exe, version));
            }
        }
    }
    None
}

pub async fn dependency_status(service: &ToolService) -> DependencyStatus {
    let settings = service.settings.snapshot();

    // Python：uv 自管优先，其次系统 Python
    let python = if service.managed_python_ready().await {
        ToolMeta {
            version: if settings.python.version.is_empty() {
                manifest::PYTHON_VERSION.to_string()
            } else {
                settings.python.version.clone()
            },
            installed: true,
            installed_at: settings.python.installed_at.clone(),
        }
    } else if let Some((_, version)) = system_python_info() {
        ToolMeta {
            version,
            installed: true,
            installed_at: None,
        }
    } else {
        ToolMeta {
            version: settings.python.version.clone(),
            installed: false,
            installed_at: settings.python.installed_at.clone(),
        }
    };

    // Git / uv / pnpm：自管目录存在即就绪，否则探测系统工具
    let tool = |kind: ToolKind| -> ToolMeta {
        let state = match kind {
            ToolKind::Python => unreachable!("python 单独处理"),
            ToolKind::Git => &settings.git,
            ToolKind::Uv => &settings.uv,
            ToolKind::Pnpm => &settings.pnpm,
        };
        if let Some(exe) = service.managed_exe(kind) {
            let version = if state.version.is_empty() {
                probe_version(&exe).unwrap_or_default()
            } else {
                state.version.clone()
            };
            ToolMeta {
                version,
                installed: true,
                installed_at: state.installed_at.clone(),
            }
        } else if let Some(version) =
            system_exe_path(kind).and_then(|exe| probe_version(&exe))
        {
            ToolMeta {
                version,
                installed: true,
                installed_at: None,
            }
        } else {
            ToolMeta {
                version: state.version.clone(),
                installed: false,
                installed_at: state.installed_at.clone(),
            }
        }
    };

    DependencyStatus {
        python,
        uv: tool(ToolKind::Uv),
        git: tool(ToolKind::Git),
        pnpm: tool(ToolKind::Pnpm),
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
