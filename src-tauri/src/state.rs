use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::manifest::ToolKind;
use crate::platform::{exe_suffix, is_windows};

#[derive(Debug, Clone)]
pub struct RuntimePaths {
    pub root: PathBuf,
    pub state_dir: PathBuf,
    pub downloads: PathBuf,
    pub staging: PathBuf,
    pub tools: PathBuf,
    pub tools_python: PathBuf,
    pub tools_uv: PathBuf,
    pub tools_git: PathBuf,
    pub tools_pnpm: PathBuf,
    pub env_aurora: PathBuf,
    pub kernel: PathBuf,
    pub kernel_aurora: PathBuf,
    pub home: PathBuf,
    pub logs: PathBuf,
}

impl RuntimePaths {
    pub fn default_user() -> Result<Self> {
        // 默认把运行目录放在家目录下（三平台统一），避免 exe 装在只读/系统目录
        // （例如 Linux 的 /usr/bin）时同级不可写。
        // 只有 exe 同级**已经存在** tool/ 时才用它——即用户主动放置的便携目录。
        let exe_dir = std::env::current_exe()
            .context("无法定位当前可执行文件")?
            .parent()
            .context("可执行文件缺少父目录")?
            .to_path_buf();
        let portable_root = exe_dir.join("tool");
        if portable_root.exists() {
            return Ok(Self::from_root(portable_root));
        }
        let data_root = user_data_root()
            .context("无法定位用户数据目录")?
            .join("tool");
        Ok(Self::from_root(data_root))
    }

    pub fn from_root(root: PathBuf) -> Self {
        // 按用途分块：runtime（运行必需）/ data（用户数据）/ cache（可删缓存）/ logs
        let runtime = root.join("runtime");
        let tools = runtime.join("tools");
        let tools_python = tools.join("python");
        let tools_uv = tools.join("uv");
        let tools_git = tools.join("git");
        let tools_pnpm = tools.join("pnpm");
        let env_aurora = runtime.join("venv");
        let kernel = runtime.join("kernel");
        let kernel_aurora = kernel.join("auroraBot");

        let data = root.join("data");
        let state_dir = data.join("state");
        let home = data.join("home");

        let cache = root.join("cache");
        let downloads = cache.join("downloads");
        let staging = cache.join("staging");

        Self {
            root: root.clone(),
            state_dir,
            downloads,
            staging,
            tools,
            tools_python,
            tools_uv,
            tools_git,
            tools_pnpm,
            env_aurora,
            kernel,
            kernel_aurora,
            home,
            logs: root.join("logs"),
        }
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        // 与 sandbox 注入的环境变量一一对应：TEMP/TMP、APPDATA、LOCALAPPDATA
        // 都指向 home 下的子目录；uv/pip 等工具会直接往 TEMP 写临时文件，
        // 目录不存在会报 “系统找不到指定的路径 (os error 3)”，故一并预建。
        let home_temp = self.home.join("temp");
        let home_appdata_roaming = self.home.join("AppData").join("Roaming");
        let home_appdata_local = self.home.join("AppData").join("Local");

        self.migrate_layout();

        for dir in [
            &self.root,
            &self.state_dir,
            &self.downloads,
            &self.staging,
            &self.tools,
            &self.tools_python,
            &self.tools_uv,
            &self.tools_git,
            &self.tools_pnpm,
            &self.env_aurora,
            &self.kernel,
            &self.home,
            &self.logs,
            &home_temp,
            &home_appdata_roaming,
            &home_appdata_local,
        ] {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("创建运行时目录失败: {}", dir.display()))?;
        }
        Ok(())
    }

    /// 沙箱子进程会注入的目录（TEMP/APPDATA、各类缓存）。uv/pip 在安装依赖时
    /// 会直接在 `TEMP` 下建临时目录，该目录不存在就报 “系统找不到指定的路径
    /// (os error 3)”。启动时虽已预建，但运行中可能被外部清理（清理软件、用户
    /// 删除、拷贝便携包时丢掉空目录等），故每次拉起子进程前补建一次；失败不
    /// 阻塞，交由子进程自行报错。
    pub fn ensure_sandbox_dirs(&self) {
        let dirs = [
            self.home.join("temp"),
            self.home.join("AppData").join("Roaming"),
            self.home.join("AppData").join("Local"),
            self.home.join(".local"),
            self.home.join("pnpm"),
            self.root.join("cache").join("uv"),
            self.root.join("cache").join("uv-tools"),
            self.root.join("cache").join("pip"),
            self.root.join("cache").join("npm"),
        ];
        for dir in dirs {
            let _ = std::fs::create_dir_all(dir);
        }
    }

    /// 把旧的平铺布局迁移到 `runtime/ + data/ + cache/` 分块布局。
    /// 同盘改名很快；失败（目录被占用等）就跳过，不阻塞启动。
    fn migrate_layout(&self) {
        let moves: [(PathBuf, PathBuf); 5] = [
            (self.root.join("tools"), self.tools.clone()),
            (self.root.join("kernel"), self.kernel.clone()),
            (self.root.join("home"), self.home.clone()),
            (self.root.join("state"), self.state_dir.clone()),
            (self.root.join("env").join("aurora"), self.env_aurora.clone()),
        ];
        for (old, new) in moves {
            if old.exists() && !new.exists() {
                if let Some(parent) = new.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::rename(&old, &new);
            }
        }
        // 更早的布局把 downloads/staging 直接放在根下
        let cache = self.root.join("cache");
        for (old_name, new_path) in [("downloads", &self.downloads), ("staging", &self.staging)] {
            let old_path = self.root.join(old_name);
            if old_path.exists() && !new_path.exists() {
                let _ = std::fs::create_dir_all(&cache);
                let _ = std::fs::rename(&old_path, new_path);
            }
        }
        // 清掉迁移后残留的空目录（非空时 remove_dir 会失败，忽略即可）
        let _ = std::fs::remove_dir(self.root.join("env"));
    }

    pub fn settings_file(&self) -> PathBuf {
        self.state_dir.join("settings.json")
    }

    /// 某工具在默认布局下的目录（未设置自定义路径时使用）。
    pub fn default_tool_dir(&self, kind: ToolKind) -> PathBuf {
        match kind {
            ToolKind::Python => self.tools_python.clone(),
            ToolKind::Uv => self.tools_uv.clone(),
            ToolKind::Git => self.tools_git.clone(),
            ToolKind::Pnpm => self.tools_pnpm.clone(),
        }
    }

    /// 某工具实际使用的目录：设置里填了自定义路径就用它，否则用默认目录。
    pub fn tool_dir(&self, kind: ToolKind, overrides: &ToolDirOverrides) -> PathBuf {
        let raw = overrides.get(kind).trim();
        if raw.is_empty() {
            self.default_tool_dir(kind)
        } else {
            PathBuf::from(raw)
        }
    }

    /// 某工具可执行文件（Python 无单一 exe，返回其目录）。
    pub fn tool_exe(&self, kind: ToolKind, overrides: &ToolDirOverrides) -> PathBuf {
        let dir = self.tool_dir(kind, overrides);
        match kind {
            ToolKind::Python => dir,
            ToolKind::Uv => dir.join(format!("uv{}", exe_suffix())),
            ToolKind::Pnpm => dir.join(format!("pnpm{}", exe_suffix())),
            ToolKind::Git => {
                if is_windows() {
                    dir.join("cmd").join("git.exe")
                } else {
                    dir.join("bin").join("git")
                }
            }
        }
    }

    pub fn env_python(&self) -> PathBuf {
        if is_windows() {
            self.env_aurora.join("Scripts").join("python.exe")
        } else {
            self.env_aurora.join("bin").join("python")
        }
    }

    /// 虚拟环境的标记文件，uv/CPython 靠它识别这是一个 venv。
    pub fn env_cfg(&self) -> PathBuf {
        self.env_aurora.join("pyvenv.cfg")
    }

    /// 虚拟环境是否完整可用。仅凭 python.exe 存在不足以判定：venv 创建被中断时
    /// 会留下只有 python.exe、没有 pyvenv.cfg 的残壳，此时 uv 会以
    /// “No pyvenv.cfg file” 直接失败，必须识别出来并重建。
    pub fn venv_ready(&self) -> bool {
        self.env_python().is_file() && self.env_cfg().is_file()
    }

    pub fn env_scripts(&self) -> PathBuf {
        if is_windows() {
            self.env_aurora.join("Scripts")
        } else {
            self.env_aurora.join("bin")
        }
    }

    pub fn tool_path_entries(&self, overrides: &ToolDirOverrides) -> Vec<PathBuf> {
        let uv = self.tool_dir(ToolKind::Uv, overrides);
        let pnpm = self.tool_dir(ToolKind::Pnpm, overrides);
        let git = self.tool_dir(ToolKind::Git, overrides);
        let mut entries = vec![uv, pnpm];
        if is_windows() {
            entries.push(git.join("cmd"));
            entries.push(git.join("mingw64").join("bin"));
            entries.push(git.join("usr").join("bin"));
        } else {
            entries.push(git.join("bin"));
        }
        entries.push(self.env_scripts());
        entries
    }
}

/// 非便携回退：家目录下的 `~/.aurora-launcher`，三平台路径一致。
fn user_data_root() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.home_dir().join(".aurora-launcher"))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolState {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub installed_at: Option<String>,
    /// 上次成功安装到的目录（用于换路径重装时清理旧副本）。
    #[serde(default)]
    pub path: String,
    /// 是否优先使用系统 PATH 上的版本（false = 优先启动器自管副本）。
    #[serde(default)]
    pub prefer_system: bool,
}

/// 每个工具的自定义安装目录覆盖；空字符串表示用默认目录。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolDirOverrides {
    #[serde(default)]
    pub python: String,
    #[serde(default)]
    pub uv: String,
    #[serde(default)]
    pub git: String,
    #[serde(default)]
    pub pnpm: String,
}

impl ToolDirOverrides {
    pub fn get(&self, kind: ToolKind) -> &str {
        match kind {
            ToolKind::Python => &self.python,
            ToolKind::Uv => &self.uv,
            ToolKind::Git => &self.git,
            ToolKind::Pnpm => &self.pnpm,
        }
    }

    pub fn set(&mut self, kind: ToolKind, value: String) {
        match kind {
            ToolKind::Python => self.python = value,
            ToolKind::Uv => self.uv = value,
            ToolKind::Git => self.git = value,
            ToolKind::Pnpm => self.pnpm = value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub python: ToolState,
    pub uv: ToolState,
    pub git: ToolState,
    pub pnpm: ToolState,
    #[serde(default)]
    pub tool_dirs: ToolDirOverrides,
    #[serde(default = "default_remote")]
    pub aurora_remote: String,
    #[serde(default = "default_branch")]
    pub aurora_branch: String,
    /// 包下载源：`mirror`（默认，国内 PyPI 镜像）或 `official`。
    #[serde(default = "default_download_source")]
    pub download_source: String,
    /// GitHub 加速前缀（如 `https://ghproxy.net/`）：非空时工具安装包与 Python
    /// 解释器都改从它下载；留空 = 直连 github.com。只影响 GitHub，不影响 PyPI。
    #[serde(default)]
    pub github_mirror: String,
    /// 创建 venv 时用的 Python 来源（`managed` / `system`）。
    /// 切换来源不会重建 venv，所以用它判断「当前选择」与「venv 实际来源」是否一致；
    /// 为空表示旧版本记录（视为未知，不触发重建）。
    #[serde(default)]
    pub venv_source: String,
}

fn default_download_source() -> String {
    "mirror".into()
}

fn default_remote() -> String {
    "https://github.com/AuroraBot-Dev/AuroraBot.git".into()
}

fn default_branch() -> String {
    "main".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            python: ToolState::default(),
            uv: ToolState::default(),
            git: ToolState::default(),
            pnpm: ToolState::default(),
            tool_dirs: ToolDirOverrides::default(),
            aurora_remote: default_remote(),
            aurora_branch: default_branch(),
            download_source: default_download_source(),
            github_mirror: String::new(),
            venv_source: String::new(),
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    inner: Mutex<Settings>,
}

impl SettingsStore {
    pub fn load(path: PathBuf) -> Self {
        let settings = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        Self {
            path,
            inner: Mutex::new(settings),
        }
    }

    pub fn snapshot(&self) -> Settings {
        self.inner.lock().unwrap().clone()
    }

    /// 包下载源：`mirror`（国内镜像）或 `official`。
    pub fn download_source(&self) -> String {
        self.inner.lock().unwrap().download_source.clone()
    }

    /// GitHub 加速前缀；空字符串表示直连 github.com。
    pub fn github_mirror(&self) -> String {
        self.inner.lock().unwrap().github_mirror.trim().to_string()
    }

    /// 该工具是否优先使用系统 PATH 上的版本（默认 false = 优先启动器自管副本）。
    pub fn tool_prefers_system(&self, kind: ToolKind) -> bool {
        let settings = self.inner.lock().unwrap();
        match kind {
            ToolKind::Python => settings.python.prefer_system,
            ToolKind::Uv => settings.uv.prefer_system,
            ToolKind::Git => settings.git.prefer_system,
            ToolKind::Pnpm => settings.pnpm.prefer_system,
        }
    }

    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> Result<()> {
        let mut guard = self.inner.lock().unwrap();
        // 先在副本上改，写盘成功后再替换内存：否则写盘失败时内存已改、磁盘没改，
        // 二者不一致，下次启动会悄悄回滚成旧值。
        let mut next = guard.clone();
        f(&mut next);
        let parent = self.path.parent().context("settings.json 缺少父目录")?;
        std::fs::create_dir_all(parent)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&next)?)?;
        std::fs::rename(&tmp, &self.path)?;
        *guard = next;
        Ok(())
    }
}

/// 工具来源：启动器自管 / 复用系统 PATH / 未安装。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    Managed,
    System,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolMeta {
    pub version: String,
    pub installed: bool,
    pub source: ToolSource,
    /// 该工具当前使用的安装目录（自定义路径或默认目录）。
    pub path: String,
    pub installed_at: Option<String>,
    /// 是否同时存在启动器副本与系统版本（供界面切换来源）。
    #[serde(default)]
    pub managed_available: bool,
    #[serde(default)]
    pub system_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyStatus {
    pub python: ToolMeta,
    /// venv 的来源与当前选中的 Python 来源不一致：需要重新初始化才会生效
    #[serde(default)]
    pub python_venv_stale: bool,
    pub uv: ToolMeta,
    pub git: ToolMeta,
    pub pnpm: ToolMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelStatus {
    pub exists: bool,
    pub remote: String,
    pub branch: String,
    pub commit_short: String,
    pub message: String,
    pub date: String,
}

impl KernelStatus {
    pub fn empty(remote: &str, branch: &str) -> Self {
        Self {
            exists: false,
            remote: remote.to_string(),
            branch: branch.to_string(),
            commit_short: String::new(),
            message: String::new(),
            date: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuroraProcessInfo {
    pub running: bool,
    pub pid: u32,
    pub started_at: String,
    pub log_file: Option<PathBuf>,
}

pub struct BotRecord {
    pub child: Child,
    pub pid: u32,
    pub started_at: String,
    pub log_file: PathBuf,
    /// Bot 的 stdin，用来把「对话」页的输入写进去。
    pub stdin: Mutex<Option<std::process::ChildStdin>>,
}

pub struct BotRegistry {
    inner: Mutex<Option<BotRecord>>,
}

impl BotRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    pub fn set(&self, record: BotRecord) {
        *self.inner.lock().unwrap() = Some(record);
    }

    pub fn info(&self) -> Option<AuroraProcessInfo> {
        let mut guard = self.inner.lock().unwrap();
        let record = guard.as_mut()?;
        if record.child.try_wait().ok().flatten().is_some() {
            *guard = None;
            return None;
        }
        Some(AuroraProcessInfo {
            running: true,
            pid: record.pid,
            started_at: record.started_at.clone(),
            log_file: Some(record.log_file.clone()),
        })
    }

    /// 往 Bot 的 stdin 写入一行输入（对话）。
    pub fn send_input(&self, text: &str) -> anyhow::Result<()> {
        use anyhow::{anyhow, Context};
        use std::io::Write;

        let mut guard = self.inner.lock().unwrap();
        let record = guard.as_mut().ok_or_else(|| anyhow!("AuroraBot 未运行"))?;
        let stdin = record.stdin.get_mut().unwrap();
        let stdin = stdin
            .as_mut()
            .ok_or_else(|| anyhow!("当前 Bot 不是对话模式，请重启 Bot"))?;
        stdin.write_all(text.as_bytes()).context("写入 Bot 输入失败")?;
        stdin.write_all(b"\n").context("写入 Bot 输入失败")?;
        stdin.flush().context("刷新 Bot 输入失败")?;
        Ok(())
    }

    pub fn stop(&self) -> Option<AuroraProcessInfo> {
        let mut guard = self.inner.lock().unwrap();
        let mut record = guard.take()?;
        let _ = record.child.kill();
        let _ = record.child.wait();
        Some(AuroraProcessInfo {
            running: false,
            pid: record.pid,
            started_at: record.started_at,
            log_file: Some(record.log_file),
        })
    }
}

#[derive(Clone)]
pub struct AppState {
    pub paths: Arc<RuntimePaths>,
    pub settings: Arc<SettingsStore>,
    pub bot: Arc<BotRegistry>,
    /// 安装互斥：同一时间只允许一个安装任务，避免并发写受管目录/暂存目录。
    pub installing: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(paths: RuntimePaths) -> Result<Self> {
        paths.ensure_dirs()?;
        let settings = SettingsStore::load(paths.settings_file());
        Ok(Self {
            paths: Arc::new(paths),
            settings: Arc::new(settings),
            bot: Arc::new(BotRegistry::new()),
            installing: Arc::new(AtomicBool::new(false)),
        })
    }
}

impl Default for BotRegistry {
    fn default() -> Self {
        Self::new()
    }
}
