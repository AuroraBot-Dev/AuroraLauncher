use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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

/// git 网络操作的最大尝试次数（含首次）与单次等待退避上限（秒）。
/// 第三方挂钩/代理软件可能瞬时阻断 https 传输；杀软或瞬时资源紧张还会让
/// curl 的 DNS 解析线程（getaddrinfo）启动失败，给足重试窗口再报错更友好。
const GIT_RETRY_ATTEMPTS: u32 = 5;
const GIT_RETRY_MAX_DELAY: u64 = 16;

/// 第 round 次（从 1 起）失败后的等待秒数：指数退避 2/4/8/16，并封顶
fn retry_backoff_seconds(round: u32) -> u64 {
    2u64.saturating_pow(round).min(GIT_RETRY_MAX_DELAY)
}

/// stderr 是否为“瞬时性”的 git 网络/传输失败；命中时在最终错误里追加排障提示
fn is_transient_git_error(stderr: &str) -> bool {
    const MARKERS: &[&str] = &[
        "getaddrinfo() thread failed to start",
        "Could not resolve host",
        "Failed to connect",
        "Connection reset",
        "recv failed",
        "Operation timed out",
        "early EOF",
        "unable to access",
    ];
    MARKERS.iter().any(|marker| stderr.contains(marker))
}

const TRANSIENT_ERROR_HINT: &str = "多为网络瞬时故障或系统资源紧张所致：\
    可稍后重试，或临时退出第三方代理/加速/杀毒软件并确认能正常访问 github.com 后再试";

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

    /// 可用的 Git：优先 launcher 自管的，其次系统 PATH 上的
    fn git_exe_resolved(&self) -> Option<PathBuf> {
        let overrides = self.settings.snapshot().tool_dirs;
        let exe = self
            .paths
            .tool_exe(crate::manifest::ToolKind::Git, &overrides);
        if exe.is_file() {
            Some(exe)
        } else {
            crate::tools::system_exe_path(crate::manifest::ToolKind::Git)
        }
    }

    /// 沙箱 git 只读受管的 <home>/.gitconfig（sandbox 用 GIT_CONFIG_GLOBAL 指向它，
    /// 并用 GIT_CONFIG_NOSYSTEM 屏蔽系统配置）。在部分网络下 schannel 的证书吊销
    /// 检查不可达、甚至到 GitHub 的 TLS 链路会被拦截，实测只有关闭证书校验才能稳定
    /// clone，故默认写入 http.sslVerify=false（与 Git 官方一致的做法之一；也贴合
    /// 常见国内/代理网络）。仅在缺失该键时补写，不覆盖已有配置。
    fn ensure_global_config(&self) -> Result<()> {
        let path = self.paths.home.join(".gitconfig");
        std::fs::create_dir_all(&self.paths.home)?;
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        if existing.contains("sslVerify") {
            return Ok(());
        }
        std::fs::write(
            &path,
            format!(
                "{existing}\n[http]\n\t# AuroraLauncher 沙箱默认：关闭证书校验以保证远端可达\n\t#（吊销服务器/到 GitHub 的 TLS 链路常被网络环境拦截）\n\tsslVerify = false\n"
            ),
        )
        .with_context(|| format!("写入沙箱 git 全局配置失败: {}", path.display()))
    }

    pub fn ensure_cloned(&self, app: &AppHandle) -> Result<KernelStatus> {
        if self.bot.info().is_some() {
            bail!("AuroraBot 正在运行，请先停止后再更新内核");
        }
        if self.is_cloned() {
            return self.status();
        }
        if self.git_exe_resolved().is_none() {
            bail!("未找到可用的 Git：既未安装受管 Git，系统 PATH 中也没有 git");
        }
        self.ensure_global_config()?;

        events::log(app, "info", "开始克隆 AuroraBot 内核");
        events::progress(app, "clone", 0, None, Some("克隆 AuroraBot 内核".into()));
        std::fs::create_dir_all(&self.paths.kernel)?;

        let settings = self.settings.snapshot();
        // git 的 https 传输在第三方挂钩/代理软件或瞬时资源紧张下可能失败，
        // 统一走带指数退避的自动重试；每次尝试前先清理上次的残留目录。
        // 克隆期间实时解析 git 进度并发给前端
        let clone_output = self.with_retry(app, "克隆 AuroraBot 内核", |attempt| {
            if self.paths.kernel_aurora.exists() {
                std::fs::remove_dir_all(&self.paths.kernel_aurora)
                    .with_context(|| format!("清理第 {attempt} 次克隆残留目录失败"))?;
            }
            self.clone_streamed(&settings.aurora_remote, &settings.aurora_branch, app)
        })?;
        ensure_success(clone_output, "git clone")?;

        let status = self.status()?;
        self.settings.update(|settings| {
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
        self.ensure_global_config()?;

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

        let fetch_args: Vec<&str> = vec![
            "fetch",
            "--depth",
            "1",
            "origin",
            settings.aurora_branch.as_str(),
        ];
        let fetch = self.git_retry(&fetch_args, Some(dir), app, "拉取内核更新")?;
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
        let submodules = self.git_retry(
            &[
                "submodule",
                "update",
                "--init",
                "--recursive",
                "--depth",
                "1",
            ],
            Some(dir),
            app,
            "更新子模块",
        )?;
        ensure_success(submodules, "git submodule update")?;

        let status = self.status()?;
        self.settings.update(|settings| {
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

    /// 运行内核自带的 `aurora about`，返回其描述文本。
    /// 优先用虚拟环境里的 aurora 控制台脚本；没有时退回 `python -m aurora.main`。
    pub fn about(&self, app: &AppHandle) -> Result<String> {
        if !self.is_cloned() {
            bail!("AuroraBot 内核尚未下载，请先下载核心");
        }
        if !self.paths.venv_ready() {
            bail!("运行环境尚未准备，请先启动一次 AuroraBot 以创建虚拟环境");
        }
        let sandbox = Sandbox::new(&self.paths, &self.settings);
        let mut cmd = match self.aurora_script() {
            Some(script) => {
                let mut cmd = sandbox.command(&script);
                cmd.current_dir(&self.paths.kernel_aurora);
                cmd
            }
            None => {
                let mut cmd = sandbox.command(&self.paths.env_python());
                cmd.current_dir(&self.paths.kernel_aurora);
                cmd.arg("-m")
                    .arg("aurora.main")
                    .arg("--root")
                    .arg(&self.paths.kernel_aurora);
                cmd
            }
        };
        cmd.arg("about");
        events::log(app, "info", "读取内核描述（aurora about）");
        let output = cmd.output().context("执行 aurora about 失败")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if stderr.is_empty() { stdout } else { stderr };
            bail!("aurora about 执行失败：{detail}");
        }
        // 只去掉末尾换行：LOGO 首行的缩进是排版的一部分，不能被 trim 掉
        let text = String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string();
        if text.is_empty() {
            bail!("aurora about 未返回任何内容");
        }
        Ok(text)
    }

    /// 虚拟环境中的 `aurora` 控制台脚本（Windows 为 aurora.exe）。
    fn aurora_script(&self) -> Option<PathBuf> {
        let candidate = self
            .paths
            .env_scripts()
            .join(format!("aurora{}", crate::platform::exe_suffix()));
        candidate.is_file().then_some(candidate)
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
        let mut cmd = self.git_command(cwd)?;
        cmd.args(args);
        let output = cmd.output().with_context(|| format!("执行 {label} 失败"))?;
        if output.status.success() {
            events::log(app, "info", format!("{label} 完成"));
        }
        Ok(output)
    }

    /// git 网络操作自动重试（第三方挂钩/代理软件可能瞬时阻断 https 传输，或瞬时
    /// 资源紧张导致 DNS 解析线程启动失败），与克隆共用同一套指数退避策略
    fn git_retry(
        &self,
        args: &[&str],
        cwd: Option<&Path>,
        app: &AppHandle,
        label: &str,
    ) -> Result<Output> {
        self.with_retry(app, label, |_| {
            self.run_git(args.iter().copied(), cwd, app, label)
        })
    }

    /// git 网络操作统一自动重试：最多 GIT_RETRY_ATTEMPTS 次，间隔指数退避封顶。
    /// 每次尝试由 attempt 闭包执行；闭包返回 Err（如清理残留目录失败）直接上抛，
    /// 返回的 Output 非成功则按策略重试，直到成功或用尽次数后返回最后一次输出。
    fn with_retry<F>(&self, app: &AppHandle, label: &str, mut attempt: F) -> Result<Output>
    where
        F: FnMut(u32) -> Result<Output>,
    {
        let mut last: Option<Output> = None;
        for round in 1..=GIT_RETRY_ATTEMPTS {
            let output = attempt(round)?;
            if output.status.success() {
                return Ok(output);
            }
            if round < GIT_RETRY_ATTEMPTS {
                let seconds = retry_backoff_seconds(round);
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                events::log(
                    app,
                    "warn",
                    format!(
                        "{label} 失败（{stderr}），{seconds} 秒后自动重试（第 {}/{} 次）",
                        round + 1,
                        GIT_RETRY_ATTEMPTS
                    ),
                );
                std::thread::sleep(std::time::Duration::from_secs(seconds));
            }
            last = Some(output);
        }
        Ok(last.expect("with_retry 至少有 1 次输出"))
    }

    /// git clone 实时进度版：加 --progress 让 git 在管道输出上也会打印
    /// “Receiving objects: NN%”，用后台线程边读边把百分比推给前端；
    /// stdout/stderr 并发读，避免管道写满互相死锁。
    fn clone_streamed(&self, remote: &str, branch: &str, app: &AppHandle) -> Result<Output> {
        let mut cmd = self.git_command(Some(&self.paths.kernel))?;
        cmd.args([
            "clone",
            "--progress",
            "--depth",
            "1",
            "--branch",
            branch,
            "--recurse-submodules",
            "--shallow-submodules",
            remote,
            "auroraBot",
        ]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        let mut child = cmd.spawn().context("启动 git clone 失败")?;
        let mut stderr = child.stderr.take().context("读取 git stderr 失败")?;
        let mut stdout = child.stdout.take().context("读取 git stdout 失败")?;

        let app_for_thread = app.clone();
        let stderr_handle = std::thread::spawn(move || -> Vec<u8> {
            let mut bytes = Vec::new();
            let mut buf = [0u8; 4096];
            // git 进度用 \r 刷新同一行，可能长时间没有换行，需要累积文本再解析
            let mut tail = String::new();
            // 主仓库之后每个子模块又是一段独立的 0→100 进度，这里把第 phase 段映射到
            // 递增区间 [1-1/2^phase, 1-1/2^(phase+1))，跨仓库整体单调爬升、不回落
            let mut phase: u32 = 0;
            let mut last_raw: i32 = -1;
            let mut last_overall: i32 = -1;
            loop {
                match stderr.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        bytes.extend_from_slice(&buf[..n]);
                        tail.push_str(&String::from_utf8_lossy(&buf[..n]));
                        if let Some(pct) = parse_progress_percent(&tail) {
                            let raw = pct as i32;
                            if last_raw >= 0 && raw < last_raw {
                                phase += 1;
                            }
                            last_raw = raw;
                            let lower = 100.0 * (1.0 - (0.5f64).powi(phase as i32));
                            let upper = 100.0 * (1.0 - (0.5f64).powi(phase as i32 + 1));
                            let overall =
                                (lower + (upper - lower) * f64::from(raw) / 100.0).round() as i32;
                            let overall = overall.min(99);
                            if overall != last_overall {
                                last_overall = overall;
                                events::progress(
                                    &app_for_thread,
                                    "clone",
                                    overall as u64,
                                    Some(100),
                                    Some("克隆 AuroraBot 内核".into()),
                                );
                            }
                            tail.clear();
                        } else if tail.len() > 8192 {
                            tail.clear();
                        }
                    }
                    Err(_) => break,
                }
            }
            bytes
        });

        let stdout_handle = std::thread::spawn(move || -> Vec<u8> {
            let mut bytes = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                match stdout.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => bytes.extend_from_slice(&buf[..n]),
                    Err(_) => break,
                }
            }
            bytes
        });

        let status = child.wait().context("等待 git clone 结束失败")?;
        let stderr_bytes = stderr_handle.join().unwrap_or_default();
        let stdout_bytes = stdout_handle.join().unwrap_or_default();
        if status.success() {
            // 阶段映射上限不到 100，真正完成后把进度顶满再交给上层清空
            events::progress(
                app,
                "clone",
                100,
                Some(100),
                Some("克隆 AuroraBot 内核".into()),
            );
        }
        Ok(Output {
            status,
            stdout: stdout_bytes,
            stderr: stderr_bytes,
        })
    }

    fn run_git_text(&self, args: &[&str], cwd: Option<&Path>) -> Option<String> {
        let mut cmd = self.git_command(cwd).ok()?;
        cmd.args(args);
        let output = cmd.output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn git_command(&self, cwd: Option<&Path>) -> Result<Command> {
        let exe = self
            .git_exe_resolved()
            .ok_or_else(|| anyhow::anyhow!("未找到可用 Git"))?;
        let mut cmd = Sandbox::new(&self.paths, &self.settings).command(&exe);
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        Ok(cmd)
    }
}

/// 从 git 进度文本里取最后一个 “NN%”（如 “Receiving objects: 42%”）。
/// git 进度以 \r 覆盖式刷新，直接扫最后一个 % 前的一串数字即可。
fn parse_progress_percent(text: &str) -> Option<u32> {
    let bytes = text.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        if bytes[i] != b'%' {
            continue;
        }
        let mut value: u32 = 0;
        let mut mul: u32 = 1;
        let mut j = i;
        while j > 0 && bytes[j - 1].is_ascii_digit() {
            // 数字串异常长（远超 git 的 3 位百分比）时直接放弃，
            // 避免溢出算出一个错误百分比
            let digit = u32::from(bytes[j - 1] - b'0');
            value = digit
                .checked_mul(mul)
                .and_then(|part| value.checked_add(part))?;
            mul = mul.saturating_mul(10);
            j -= 1;
        }
        if mul > 1 {
            return Some(value);
        }
    }
    None
}

pub fn ensure_success(output: Output, label: &str) -> Result<()> {
    if output.status.success() {
        Ok(())
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let mut message = format!("{label} 失败：\nstdout={stdout}\nstderr={stderr}");
        if is_transient_git_error(&stderr) {
            message.push('\n');
            message.push_str(TRANSIENT_ERROR_HINT);
        }
        bail!("{message}")
    }
}
