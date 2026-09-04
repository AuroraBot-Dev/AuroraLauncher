use std::path::Path;
use std::process::{Command, Output};

use anyhow::{bail, Context, Result};
use tauri::AppHandle;

use crate::events;
use crate::sandbox::Sandbox;
use crate::state::{AppState, BotRegistry, KernelStatus, RuntimePaths, SettingsStore};

#[derive(Clone)]
pub struct KernelService {
    pub paths: std::sync::Arc<RuntimePaths>,
    pub settings: std::sync::Arc<SettingsStore>,
    pub bot: std::sync::Arc<BotRegistry>,
}

impl KernelService {
    pub fn new(state: &AppState) -> Self {
        Self {
            paths: state.paths.clone(),
            settings: state.settings.clone(),
            bot: state.bot.clone(),
        }
    }

    pub fn is_cloned(&self) -> bool {
        self.paths.kernel_aurora.join(".git").exists()
            && self.paths.kernel_aurora.join("pyproject.toml").exists()
    }

    pub fn ensure_cloned(&self, app: &AppHandle) -> Result<KernelStatus> {
        if self.bot.info().is_some() {
            bail!("AuroraBot 正在运行，请先停止后再更新内核");
        }
        if self.is_cloned() {
            return self.status();
        }
        if !self.paths.git_exe().exists() {
            bail!("受管 Git 尚未安装，请先安装 Git");
        }

        events::log(app, "info", "开始克隆 AuroraBot 内核");
        events::progress(app, "clone", 0, None, Some("克隆 AuroraBot 内核".into()));
        std::fs::create_dir_all(&self.paths.kernel)?;
        if self.paths.kernel_aurora.exists() {
            std::fs::remove_dir_all(&self.paths.kernel_aurora)
                .context("清理不完整的内核目录失败")?;
        }

        let settings = self.settings.snapshot();
        let output = self.run_git(
            [
                "clone",
                "--depth",
                "1",
                "--branch",
                &settings.aurora_branch,
                "--recurse-submodules",
                "--shallow-submodules",
                &settings.aurora_remote,
                "auroraBot",
            ]
            .into_iter(),
            Some(&self.paths.kernel),
            app,
            "克隆 AuroraBot 内核",
        )?;
        ensure_success(output, "git clone")?;

        let status = self.status()?;
        self.settings.update(|settings| {
            settings.aurora_commit = status.commit.clone();
            settings.aurora_remote = status.remote.clone();
            settings.aurora_branch = status.branch.clone();
        })?;
        events::progress(app, "clone", 0, None, None);
        events::log(
            app,
            "success",
            format!("内核已克隆到 {}", status.commit_short),
        );
        Ok(status)
    }

    pub fn update(&self, app: &AppHandle) -> Result<KernelStatus> {
        if self.bot.info().is_some() {
            bail!("AuroraBot 正在运行，请先停止后再更新内核");
        }
        if !self.is_cloned() {
            return self.ensure_cloned(app);
        }

        let dir = &self.paths.kernel_aurora;
        let settings = self.settings.snapshot();
        events::log(app, "info", "开始检查 AuroraBot 更新");
        events::progress(app, "update", 0, None, Some("检查内核更新".into()));

        let dirty = self.run_git(
            ["status", "--porcelain", "--untracked-files=no"].into_iter(),
            Some(dir),
            app,
            "检查内核本地修改",
        )?;
        if !String::from_utf8_lossy(&dirty.stdout).trim().is_empty() {
            bail!(
                "内核目录存在本地修改，launcher 不会覆盖：\n{}",
                String::from_utf8_lossy(&dirty.stdout)
            );
        }

        let old_head = self.run_git(
            ["rev-parse", "HEAD"].into_iter(),
            Some(dir),
            app,
            "读取当前提交",
        )?;
        let old_head = String::from_utf8_lossy(&old_head.stdout).trim().to_string();

        let _ = self.run_git(
            ["remote", "set-url", "origin", &settings.aurora_remote].into_iter(),
            Some(dir),
            app,
            "设置内核远端",
        );

        let fetch = self.run_git(
            ["fetch", "--depth", "1", "origin", &settings.aurora_branch].into_iter(),
            Some(dir),
            app,
            "拉取内核更新",
        )?;
        ensure_success(fetch, "git fetch")?;

        let new_head = self.run_git(
            ["rev-parse", "FETCH_HEAD"].into_iter(),
            Some(dir),
            app,
            "读取远端提交",
        )?;
        let new_head = String::from_utf8_lossy(&new_head.stdout).trim().to_string();
        if new_head != old_head {
            let reset = self.run_git(
                ["reset", "--hard", "FETCH_HEAD"].into_iter(),
                Some(dir),
                app,
                "切换到新提交",
            )?;
            ensure_success(reset, "git reset")?;
        }

        let sync = self.run_git(
            ["submodule", "sync", "--recursive"].into_iter(),
            Some(dir),
            app,
            "同步子模块远端",
        );
        if let Ok(sync) = sync {
            ensure_success(sync, "git submodule sync")?;
        }
        let submodules = self.run_git(
            [
                "submodule",
                "update",
                "--init",
                "--recursive",
                "--depth",
                "1",
            ]
            .into_iter(),
            Some(dir),
            app,
            "更新子模块",
        )?;
        ensure_success(submodules, "git submodule update")?;

        let status = self.status()?;
        self.settings.update(|settings| {
            settings.aurora_commit = status.commit.clone();
            settings.aurora_remote = status.remote.clone();
            settings.aurora_branch = status.branch.clone();
        })?;
        events::progress(app, "update", 0, None, None);
        events::log(
            app,
            "success",
            if new_head == old_head {
                format!("内核已是最新：{}", status.commit_short)
            } else {
                format!("内核已更新到 {}", status.commit_short)
            },
        );
        Ok(status)
    }

    pub fn status(&self) -> Result<KernelStatus> {
        let settings = self.settings.snapshot();
        if !self.is_cloned() {
            return Ok(KernelStatus::empty(
                &settings.aurora_remote,
                &settings.aurora_branch,
            ));
        }

        let dir = &self.paths.kernel_aurora;
        let text = |args: &[&str]| -> String {
            self.run_git_text(args, Some(dir))
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let commit = text(&["rev-parse", "HEAD"]);
        let commit_short = text(&["rev-parse", "--short=7", "HEAD"]);
        let message = text(&["log", "-1", "--pretty=%s"]);
        let date = text(&["log", "-1", "--pretty=%ci"]);
        let remote = text(&["remote", "get-url", "origin"]);
        let branch = text(&["rev-parse", "--abbrev-ref", "HEAD"]);
        Ok(KernelStatus {
            exists: true,
            remote: if remote.is_empty() {
                settings.aurora_remote
            } else {
                remote
            },
            branch: if branch.is_empty() {
                settings.aurora_branch
            } else {
                branch
            },
            commit,
            commit_short,
            message,
            date,
        })
    }

    fn run_git<'a>(
        &self,
        args: impl Iterator<Item = &'a str>,
        cwd: Option<&Path>,
        app: &AppHandle,
        label: &str,
    ) -> Result<Output> {
        let mut cmd = self.git_command(cwd);
        cmd.args(args);
        let output = cmd.output().with_context(|| format!("执行 {label} 失败"))?;
        events::log(app, "info", format!("{label} 完成"));
        Ok(output)
    }

    fn run_git_text(&self, args: &[&str], cwd: Option<&Path>) -> Option<String> {
        let mut cmd = self.git_command(cwd);
        cmd.args(args);
        let output = cmd.output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn git_command(&self, cwd: Option<&Path>) -> Command {
        let mut cmd = Sandbox::new(&self.paths).command(&self.paths.git_exe());
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        cmd
    }
}

pub fn ensure_success(output: Output, label: &str) -> Result<()> {
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
