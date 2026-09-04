use std::fs::File;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use anyhow::{bail, Context, Result};
use chrono::Local;
use tauri::{AppHandle, Emitter};

use crate::events;
use crate::kernel::KernelService;
use crate::manifest::ToolKind;
use crate::sandbox::Sandbox;
use crate::state::{
    AppState, AuroraProcessInfo, BotRecord, BotRegistry, RuntimePaths, SettingsStore,
};
use crate::tools::ToolService;

#[derive(Clone)]
pub struct BotService {
    pub paths: std::sync::Arc<RuntimePaths>,
    pub settings: std::sync::Arc<SettingsStore>,
    pub bot: std::sync::Arc<BotRegistry>,
}

impl BotService {
    pub fn new(state: &AppState) -> Self {
        Self {
            paths: state.paths.clone(),
            settings: state.settings.clone(),
            bot: state.bot.clone(),
        }
    }

    pub async fn prepare(&self, app: &AppHandle) -> Result<()> {
        let tools = ToolService {
            paths: self.paths.clone(),
            settings: self.settings.clone(),
            bot: self.bot.clone(),
        };
        if !self.paths.git_exe().exists() {
            tools.install(ToolKind::Git, app).await?;
        }
        if !self.paths.uv_exe().exists() {
            tools.install(ToolKind::Uv, app).await?;
        }
        if !tools.is_installed(ToolKind::Python).await {
            tools.install(ToolKind::Python, app).await?;
        }

        let kernel = KernelService {
            paths: self.paths.clone(),
            settings: self.settings.clone(),
            bot: self.bot.clone(),
        };
        if !kernel.is_cloned() {
            kernel.ensure_cloned(app)?;
        }

        let root = &self.paths.kernel_aurora;
        copy_template_if_missing(&root.join("config.example"), &root.join("config"))?;
        copy_file_if_missing(&root.join(".env.example"), &root.join(".env"))?;

        if !self.paths.env_python().exists() {
            events::log(app, "info", "创建 AuroraBot Python 虚拟环境");
            let mut venv = Sandbox::new(&self.paths).command(&self.paths.uv_exe());
            venv.arg("venv")
                .arg("--python")
                .arg(crate::manifest::PYTHON_VERSION)
                .arg(&self.paths.env_aurora);
            let output = capture_output(venv, "uv venv").await?;
            ensure_success(output, "uv venv")?;
        }

        events::log(app, "info", "同步 AuroraBot Python 依赖");
        events::progress(app, "sync", 0, None, Some("同步 Python 依赖".into()));
        let mut sync = Sandbox::new(&self.paths).command(&self.paths.uv_exe());
        sync.arg("sync").arg("--active").arg("--project").arg(root);
        let output = capture_output(sync, "uv sync").await?;
        ensure_success(output, "uv sync")?;
        events::progress(app, "sync", 0, None, None);
        events::log(app, "success", "Python 依赖已同步");
        Ok(())
    }

    pub async fn start(&self, headless: bool, app: &AppHandle) -> Result<AuroraProcessInfo> {
        if self.bot.info().is_some() {
            bail!("AuroraBot 已在运行");
        }
        self.prepare(app).await?;

        let kernel = &self.paths.kernel_aurora;
        if !self.paths.env_python().exists() {
            bail!("虚拟环境不存在：{}", self.paths.env_python().display());
        }
        if !kernel.join("aurora").join("main.py").exists() {
            bail!("内核入口不存在：{}", kernel.join("aurora").display());
        }

        let log_dir = &self.paths.logs;
        std::fs::create_dir_all(log_dir)?;
        let log_file = log_dir.join(format!(
            "aurora-{}.log",
            Local::now().format("%Y%m%d-%H%M%S")
        ));
        let output_file = File::create(&log_file).context("创建 Bot 日志失败")?;

        let mut cmd = Sandbox::new(&self.paths).command(&self.paths.env_python());
        cmd.current_dir(kernel);
        cmd.arg("-m")
            .arg("aurora.main")
            .arg("--root")
            .arg(kernel)
            .arg("start");
        if headless {
            cmd.arg("--headless");
        }
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::from(output_file.try_clone()?));
        cmd.stderr(Stdio::from(output_file));
        let child = cmd.spawn().context("启动 AuroraBot 失败")?;
        let pid = child.id();
        let started_at = Local::now().to_rfc3339();
        self.bot.set(BotRecord {
            child,
            pid,
            started_at: started_at.clone(),
            log_file: log_file.clone(),
        });

        let info = AuroraProcessInfo {
            running: true,
            pid,
            started_at,
            log_file: Some(log_file),
        };
        events::log(app, "success", format!("AuroraBot 已启动 pid={pid}"));
        emit_bot_state(app, info.clone());
        Ok(info)
    }

    pub fn stop(&self, app: &AppHandle) -> Result<AuroraProcessInfo> {
        let info = self
            .bot
            .stop()
            .ok_or_else(|| anyhow::anyhow!("当前没有运行中的 AuroraBot 进程"))?;
        events::log(app, "success", "AuroraBot 已停止");
        emit_bot_state(app, info.clone());
        Ok(info)
    }

    pub fn info(&self) -> Option<AuroraProcessInfo> {
        self.bot.info()
    }
}

fn emit_bot_state(app: &AppHandle, info: AuroraProcessInfo) {
    let _ = app.emit("bot-state-changed", info);
}

fn copy_template_if_missing(template: &Path, target: &Path) -> Result<()> {
    if target.exists() {
        return Ok(());
    }
    if !template.exists() {
        bail!("配置模板不存在：{}", template.display());
    }
    copy_tree(template, target)
}

fn copy_file_if_missing(source: &Path, target: &Path) -> Result<()> {
    if target.exists() {
        return Ok(());
    }
    if !source.exists() {
        bail!("模板文件不存在：{}", source.display());
    }
    std::fs::copy(source, target)?;
    Ok(())
}

fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn ensure_success(output: Output, label: &str) -> Result<()> {
    if output.status.success() {
        Ok(())
    } else {
        bail!(
            "{label} 失败：\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }
}

async fn capture_output(mut cmd: Command, label: &'static str) -> Result<Output> {
    tokio::task::spawn_blocking(move || cmd.output().with_context(|| format!("执行 {label} 失败")))
        .await
        .with_context(|| format!("{label} 任务失败"))?
}
