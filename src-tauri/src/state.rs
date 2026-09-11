use std::path::{Path, PathBuf};
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
    pub env: PathBuf,
    pub env_aurora: PathBuf,
    pub kernel: PathBuf,
    pub kernel_aurora: PathBuf,
    pub home: PathBuf,
    pub logs: PathBuf,
}

impl RuntimePaths {
    pub fn default_user() -> Result<Self> {
        // 便携模式：整套运行目录放在 exe 同级的 tool/ 下，随 exe 一起拷贝即迁移。
        // 若 exe 位于只读/系统目录（例如 Linux 包管理器装到 /usr/bin），
        // 同级不可写，则退回到用户数据目录，绝不为写盘而索取 root 权限。
        let exe_dir = std::env::current_exe()
            .context("无法定位当前可执行文件")?
            .parent()
            .context("可执行文件缺少父目录")?
            .to_path_buf();
        let portable_root = exe_dir.join("tool");
        if dir_writable(&portable_root) {
            return Ok(Self::from_root(portable_root));
        }
        let data_root = user_data_root()
            .context("可执行文件目录不可写，且无法定位用户数据目录")?
            .join("tool");
        Ok(Self::from_root(data_root))
    }

    pub fn from_root(root: PathBuf) -> Self {
        let state_dir = root.join("state");
        let downloads = root.join("downloads");
        let staging = root.join("staging");
        let tools = root.join("tools");
        let tools_python = tools.join("python");
        let tools_uv = tools.join("uv");
        let tools_git = tools.join("git");
        let tools_pnpm = tools.join("pnpm");
        let env = root.join("env");
        let env_aurora = env.join("aurora");
        let kernel = root.join("kernel");
        let kernel_aurora = kernel.join("auroraBot");
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
            env,
            env_aurora,
            kernel,
            kernel_aurora,
            home: root.join("home"),
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
            &self.env,
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

/// 判断目录可创建且可写。用临时文件探测，兼容属主非当前用户但仍有写权限的情况。
fn dir_writable(dir: &Path) -> bool {
    if dir.exists() {
        return probe_writable(dir);
    }
    // 目录尚不存在：先确认父目录可写，再尝试创建。
    match dir.parent() {
        Some(parent) if probe_writable(parent) => std::fs::create_dir_all(dir).is_ok(),
        _ => false,
    }
}

fn probe_writable(dir: &Path) -> bool {
    let probe = dir.join(".aurora-write-probe");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
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

    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> Result<()> {
        let mut settings = self.inner.lock().unwrap();
        f(&mut settings);
        let parent = self.path.parent().context("settings.json 缺少父目录")?;
        std::fs::create_dir_all(parent)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&*settings)?)?;
        std::fs::rename(&tmp, &self.path)?;
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyStatus {
    pub python: ToolMeta,
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
