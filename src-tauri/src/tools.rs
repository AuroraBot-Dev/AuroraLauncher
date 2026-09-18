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
    ToolSource, ToolState,
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

    /// 实际使用的工具路径：默认优先 launcher 自管的，设置里选了“系统版本”则系统优先。
    pub fn resolve_exe(&self, kind: ToolKind) -> Option<PathBuf> {
        match kind {
            ToolKind::Python => self.system_python(),
            _ => {
                if self.settings.tool_prefers_system(kind) {
                    system_exe_path(kind).or_else(|| self.managed_exe(kind))
                } else {
                    self.managed_exe(kind).or_else(|| system_exe_path(kind))
                }
            }
        }
    }

    pub fn uv_command_exe(&self) -> Option<PathBuf> {
        self.resolve_exe(ToolKind::Uv)
    }

    /// “定位”用的实际安装目录：自管目录优先，其次系统可执行文件所在目录。
    /// 缺失时返回 None（前端据此隐藏定位入口）。
    pub fn locate_dir(&self, kind: ToolKind) -> Option<PathBuf> {
        let overrides = self.settings.snapshot().tool_dirs;
        let managed = self.paths.tool_dir(kind, &overrides);
        let managed_ready = match kind {
            // Python 由 uv 管理、没有单一 exe，只要目录里有内容即视为自管就绪
            ToolKind::Python => dir_has_entries(&managed),
            _ => self.managed_exe(kind).is_some(),
        };
        // Python 用 system_python()：它会跳过微软商店占位符，保证拿到的路径可用
        let system_dir = if kind == ToolKind::Python {
            self.system_python()
                .map(|exe| exe.parent().map(Path::to_path_buf).unwrap_or(exe))
        } else {
            system_exe_path(kind).map(|exe| exe.parent().map(Path::to_path_buf).unwrap_or(exe))
        };
        // 按来源偏好返回真实使用的位置：选了系统版本就用系统目录，否则自管目录优先
        if self.settings.tool_prefers_system(kind) {
            system_dir.or(managed_ready.then_some(managed))
        } else if managed_ready {
            Some(managed)
        } else {
            system_dir
        }
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
            let target = self.ensure_tool_dir(kind);
            let default_dir = self.paths.default_tool_dir(kind);

            // 换路径重装时，把旧位置（默认目录、上次安装目录）里由启动器自管的副本
            // 一并删除，避免残留；系统 PATH 上的依赖不属于受管目录，从不触碰。
            let mut stale = vec![default_dir];
            if let Some(previous) = self.previous_installed_dir(kind) {
                stale.push(previous);
            }
            for dir in stale {
                if dir != target && dir.exists() {
                    clean_dir(&dir).with_context(|| {
                        format!("无法清理 {} 的旧受管目录: {}", kind, dir.display())
                    })?;
                }
            }

            clean_dir(&target).with_context(|| {
                format!(
                    "无法清理 {} 的受管目录，可能是仍有旧进程占用：请先在底部按钮停止 Bot 后重试",
                    kind
                )
            })?;
            std::fs::create_dir_all(&target)?;
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

    async fn install_python(&self, app: &AppHandle) -> Result<()> {
        if self.uv_command_exe().is_none() {
            self.install_uv(app).await?;
        }
        let uv = self
            .uv_command_exe()
            .ok_or_else(|| anyhow!("无法定位可用的 uv，无法安装 Python"))?;
        let requirement = manifest::requirement(ToolKind::Python)?;
        let python_dir = self.paths.tool_dir(ToolKind::Python, &self.settings.snapshot().tool_dirs);
        std::fs::create_dir_all(&python_dir)?;
        let mut cmd = Sandbox::new(&self.paths, &self.settings).command(&uv);
        cmd.arg("python")
            .arg("install")
            .arg("--install-dir")
            .arg(&python_dir)
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
                    path: python_dir.display().to_string(),
                    prefer_system: false,
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
            // 只改实际下载用的地址；记录到设置里的仍是官方 URL，方便日后换前缀
            let url = manifest::apply_github_mirror(&package.url, &self.settings.github_mirror());
            download_verified(&url, &archive, package.sha256.as_deref(), move |progress| {
                events::progress_download(
                    &app_for_progress,
                    kind_label.clone(),
                    progress.current,
                    (progress.total > 0).then_some(progress.total),
                    Some(format!("下载 {}", kind_label)),
                    progress.speed,
                );
            })
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

        // 自定义路径可能在其他盘符，rename 跨卷会失败，这里统一走 move_dir 兜底复制
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建工具目录失败: {}", parent.display()))?;
        }
        let backup = self.paths.staging.join(format!("{kind}-backup"));
        clean_dir(&backup)?;
        if target.exists() {
            move_dir(&target, &backup)?;
        }
        move_dir(&staging, &target)?;
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
                    path: target.display().to_string(),
                    prefer_system: false,
                },
            );
        })?;

        let mut verify = Sandbox::new(&self.paths, &self.settings).command(&self.exe_path(kind));
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

    /// uv 自管 Python 的解释器路径（不含系统 Python）；缺失或未安装时返回 None
    pub async fn managed_python_path(&self) -> Option<PathBuf> {
        let uv = self.uv_command_exe()?;
        let requirement = manifest::requirement(ToolKind::Python).ok()?;
        let mut cmd = Sandbox::new(&self.paths, &self.settings).command(&uv);
        cmd.arg("python")
            .arg("find")
            .arg("--no-project")
            .arg("--python-preference")
            .arg("only-managed")
            .arg(requirement.version);
        let output = capture_output(cmd, "uv python find").await.ok()?;
        if !output.status.success() {
            return None;
        }
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            None
        } else {
            Some(PathBuf::from(path))
        }
    }

    /// 仅 launcher 用 uv 自管的 Python 是否就绪（不含系统 Python）
    pub async fn managed_python_ready(&self) -> bool {
        self.managed_python_path().await.is_some()
    }

    /// Python 是否可用：自管或系统已装均可
    async fn python_ready(&self) -> bool {
        if self.managed_python_ready().await {
            return true;
        }
        system_python_info().is_some()
    }

    fn ensure_tool_dir(&self, kind: ToolKind) -> PathBuf {
        self.paths.tool_dir(kind, &self.settings.snapshot().tool_dirs)
    }

    /// 上次成功安装到的受管目录（旧版设置未记录 path 时返回 None）。
    fn previous_installed_dir(&self, kind: ToolKind) -> Option<PathBuf> {
        let settings = self.settings.snapshot();
        let state = match kind {
            ToolKind::Python => &settings.python,
            ToolKind::Uv => &settings.uv,
            ToolKind::Git => &settings.git,
            ToolKind::Pnpm => &settings.pnpm,
        };
        let raw = state.path.trim();
        (!raw.is_empty()).then(|| PathBuf::from(raw))
    }

    pub fn exe_path(&self, kind: ToolKind) -> PathBuf {
        self.paths.tool_exe(kind, &self.settings.snapshot().tool_dirs)
    }

    fn exe_name(&self, kind: ToolKind) -> String {
        match kind {
            ToolKind::Uv => format!("uv{}", if is_windows() { ".exe" } else { "" }),
            ToolKind::Pnpm => format!("pnpm{}", if is_windows() { ".exe" } else { "" }),
            ToolKind::Git | ToolKind::Python => String::new(),
        }
    }
}

fn set_tool_state(settings: &mut Settings, kind: ToolKind, mut state: ToolState) {
    let target = match kind {
        ToolKind::Python => &mut settings.python,
        ToolKind::Uv => &mut settings.uv,
        ToolKind::Git => &mut settings.git,
        ToolKind::Pnpm => &mut settings.pnpm,
    };
    // 安装/重装不改变用户选择的来源偏好
    state.prefer_system = target.prefer_system;
    *target = state;
}

/// 在系统 PATH 中查找可执行文件。
/// Windows：必须带 PATHEXT 扩展名才算可执行（目录里可能有无用的裸名 bash shim）；
/// 未命中时再补查 npm / uv 常见安装目录，兼容工具不在 GUI 会话 PATH 里的情况。
pub(crate) fn which_in_path(name: &str) -> Option<PathBuf> {
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
            // Microsoft Store 的 App Execution Alias（…\WindowsApps\python.exe）
            // 能跑 --version，但 uv 检查它时会返回 invalid response 而失败，直接跳过。
            if is_windows() && exe.to_string_lossy().contains(r"\WindowsApps\") {
                continue;
            }
            if let Some(version) = probe_version(&exe) {
                return Some((exe, version));
            }
        }
    }
    None
}

/// 从 venv 的 `pyvenv.cfg` 推断它当初由哪个来源创建（`managed` / `system`）。
///
/// 旧版本没有记录 `venv_source` 时用它补上：`pyvenv.cfg` 的 `home` 指向基础解释器目录，
/// 落在启动器自管的 Python 目录里就是 `managed`，否则是 `system`。
/// 推断不出来（文件缺失、没有 `home` 字段等）返回 `None`，此时既不提示也不重建。
pub fn infer_venv_source(service: &ToolService) -> Option<&'static str> {
    let text = std::fs::read_to_string(service.paths.env_cfg()).ok()?;
    let home = text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "home").then(|| value.trim().to_string())
    })?;
    let overrides = service.settings.snapshot().tool_dirs;
    let managed = service.paths.tool_dir(ToolKind::Python, &overrides);
    // Windows 路径大小写不敏感，统一小写 + 反斜杠再比前缀
    let normalize = |path: &str| path.replace('/', "\\").to_ascii_lowercase();
    let managed = normalize(&managed.to_string_lossy());
    let home = normalize(&home);
    Some(if home.starts_with(&managed) {
        "managed"
    } else {
        "system"
    })
}

/// venv 实际使用的 Python 来源：优先用设置里的记录值，旧版本没记录时从 `pyvenv.cfg` 推断。
pub fn venv_source(service: &ToolService) -> Option<String> {
    let recorded = service.settings.snapshot().venv_source.trim().to_string();
    if !recorded.is_empty() {
        return Some(recorded);
    }
    infer_venv_source(service).map(str::to_string)
}

/// 当前设置下 Python 应该来自哪个来源（`managed` / `system`）。
/// 必须与 `bot.rs::setup` 建 venv 时的分支判定保持一致，否则会误判“需要重建”。
pub async fn python_source(service: &ToolService) -> &'static str {
    let prefer_system = service.settings.tool_prefers_system(ToolKind::Python);
    let system_ok = service.system_python().is_some();
    let managed_ok = service.managed_python_ready().await;
    if (prefer_system && system_ok) || !managed_ok {
        "system"
    } else {
        "managed"
    }
}

pub async fn dependency_status(service: &ToolService) -> DependencyStatus {
    let settings = service.settings.snapshot();

    // Python：默认自管优先，设置里选了“系统版本”则系统优先
    let python_path = service
        .locate_dir(ToolKind::Python)
        .unwrap_or_else(|| service.paths.tool_dir(ToolKind::Python, &settings.tool_dirs))
        .display()
        .to_string();
    // 虚拟环境里实际使用的解释器版本（真正跑 Bot 的那个）。记录值/manifest 可能和它对不上，
    // 所以只要 venv 存在就以探测结果为准，保证界面显示的版本与实际一致。
    let venv_version = if service.paths.env_python().is_file() {
        probe_version(&service.paths.env_python())
    } else {
        None
    };
    let managed_python = service.managed_python_path().await;
    let managed_version = managed_python.as_deref().and_then(probe_version);
    let managed_ok = managed_python.is_some();
    let system_python = system_python_info();
    let system_ok = system_python.is_some();
    let use_system_python = pick_system(settings.python.prefer_system, managed_ok, system_ok);
    let python = if managed_ok && !use_system_python {
        ToolMeta {
            version: managed_version.or(venv_version.clone()).unwrap_or_else(|| {
                if settings.python.version.is_empty() {
                    manifest::PYTHON_VERSION.to_string()
                } else {
                    settings.python.version.clone()
                }
            }),
            installed: true,
            source: ToolSource::Managed,
            path: python_path.clone(),
            installed_at: settings.python.installed_at.clone(),
            managed_available: managed_ok,
            system_available: system_ok,
        }
    } else if system_ok {
        ToolMeta {
            version: system_python
                .map(|(_, version)| version)
                .or_else(|| venv_version.clone())
                .unwrap_or_default(),
            installed: true,
            source: ToolSource::System,
            path: python_path.clone(),
            installed_at: None,
            managed_available: managed_ok,
            system_available: system_ok,
        }
    } else {
        ToolMeta {
            version: settings.python.version.clone(),
            installed: false,
            source: ToolSource::Missing,
            path: python_path.clone(),
            installed_at: settings.python.installed_at.clone(),
            managed_available: managed_ok,
            system_available: system_ok,
        }
    };

    // Git / uv / pnpm：默认自管优先，设置里选了“系统版本”则系统优先
    let tool = |kind: ToolKind| -> ToolMeta {
        let state = match kind {
            ToolKind::Python => unreachable!("python 单独处理"),
            ToolKind::Git => &settings.git,
            ToolKind::Uv => &settings.uv,
            ToolKind::Pnpm => &settings.pnpm,
        };
        let path = service
            .locate_dir(kind)
            .unwrap_or_else(|| service.paths.tool_dir(kind, &settings.tool_dirs))
            .display()
            .to_string();
        let managed = service.managed_exe(kind);
        let managed_ok = managed.is_some();
        let system_version = system_exe_path(kind).and_then(|exe| probe_version(&exe));
        let system_ok = system_version.is_some();
        let use_system = pick_system(state.prefer_system, managed_ok, system_ok);
        if managed_ok && !use_system {
            // 以实际探测到的版本为准，记录值只作为探测失败时的回退
            let version = managed
                .as_deref()
                .and_then(probe_version)
                .unwrap_or_else(|| state.version.clone());
            ToolMeta {
                version,
                installed: true,
                source: ToolSource::Managed,
                path,
                installed_at: state.installed_at.clone(),
                managed_available: managed_ok,
                system_available: system_ok,
            }
        } else if let Some(version) = system_version {
            ToolMeta {
                version,
                installed: true,
                source: ToolSource::System,
                path,
                installed_at: None,
                managed_available: managed_ok,
                system_available: system_ok,
            }
        } else {
            ToolMeta {
                version: state.version.clone(),
                installed: false,
                source: ToolSource::Missing,
                path,
                installed_at: state.installed_at.clone(),
                managed_available: managed_ok,
                system_available: system_ok,
            }
        }
    };

    // 来源不一致只做提示，重建由用户点「初始化」触发（见 bot.rs::setup 的 rebuild_stale_venv）
    let desired_source = python_source(service).await;
    let python_venv_stale = service.paths.venv_ready()
        && venv_source(service).is_some_and(|recorded| recorded != desired_source);

    DependencyStatus {
        python,
        python_venv_stale,
        uv: tool(ToolKind::Uv),
        git: tool(ToolKind::Git),
        pnpm: tool(ToolKind::Pnpm),
    }
}

/// 是否使用系统版本：系统存在，且（用户选了系统版本 或 没有自管副本）时才用系统。
fn pick_system(prefer_system: bool, managed_ok: bool, system_ok: bool) -> bool {
    system_ok && (prefer_system || !managed_ok)
}

fn dir_has_entries(path: &Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

fn clean_dir(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// 移动目录：优先原子 rename，跨卷失败时退化为复制后删除。
fn move_dir(src: &Path, dst: &Path) -> Result<()> {
    match std::fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_dir(src, dst)?;
            std::fs::remove_dir_all(src)?;
            Ok(())
        }
    }
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
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

#[cfg(test)]
mod tests {
    use super::pick_system;

    #[test]
    fn pick_system_logic() {
        // 默认：自管优先，没有自管才用系统
        assert!(!pick_system(false, true, true));
        assert!(pick_system(false, false, true));
        assert!(!pick_system(false, true, false));
        assert!(!pick_system(false, false, false));
        // 选了“系统版本”：系统优先，没有系统才回退自管
        assert!(pick_system(true, true, true));
        assert!(pick_system(true, false, true));
        assert!(!pick_system(true, true, false));
        assert!(!pick_system(true, false, false));
    }
}
