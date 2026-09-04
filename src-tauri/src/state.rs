use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

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
        let project = ProjectDirs::from("io", "AuroraBot", "AuroraLauncher")
            .context("无法解析 AuroraLauncher 数据目录")?;
        let data = project.data_dir().to_path_buf();
        Ok(Self::from_root(data.join("runtime")))
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
        ] {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("创建运行时目录失败: {}", dir.display()))?;
        }
        Ok(())
    }

    pub fn settings_file(&self) -> PathBuf {
        self.state_dir.join("settings.json")
    }

    pub fn uv_exe(&self) -> PathBuf {
        self.tools_uv.join(format!("uv{}", exe_suffix()))
    }

    pub fn git_exe(&self) -> PathBuf {
        if is_windows() {
            self.tools_git.join("cmd").join("git.exe")
        } else {
            self.tools_git.join("bin").join("git")
        }
    }

    pub fn pnpm_exe(&self) -> PathBuf {
        self.tools_pnpm.join(format!("pnpm{}", exe_suffix()))
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

    pub fn tool_path_entries(&self) -> Vec<PathBuf> {
        let mut entries = vec![self.tools_uv.clone(), self.tools_pnpm.clone()];
        if is_windows() {
            entries.push(self.tools_git.join("cmd"));
            entries.push(self.tools_git.join("mingw64").join("bin"));
            entries.push(self.tools_git.join("usr").join("bin"));
            entries.push(self.env_scripts());
        } else {
            entries.push(self.tools_git.join("bin"));
            entries.push(self.env_scripts());
        }
        entries
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub python: ToolState,
    pub uv: ToolState,
    pub git: ToolState,
    pub pnpm: ToolState,
    #[serde(default = "default_remote")]
    pub aurora_remote: String,
    #[serde(default = "default_branch")]
    pub aurora_branch: String,
    #[serde(default)]
    pub aurora_commit: String,
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
            aurora_remote: default_remote(),
            aurora_branch: default_branch(),
            aurora_commit: String::new(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolMeta {
    pub version: String,
    pub installed: bool,
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
    pub commit: String,
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
            commit: String::new(),
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
}

impl AppState {
    pub fn new(paths: RuntimePaths) -> Result<Self> {
        paths.ensure_dirs()?;
        let settings = SettingsStore::load(paths.settings_file());
        Ok(Self {
            paths: Arc::new(paths),
            settings: Arc::new(settings),
            bot: Arc::new(BotRegistry::new()),
        })
    }
}

impl Default for BotRegistry {
    fn default() -> Self {
        Self::new()
    }
}
