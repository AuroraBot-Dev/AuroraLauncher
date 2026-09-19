use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
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

/// 对话模式的包装脚本：注入逐行 `readline`/`output`，让内核本地终端能在管道上工作。
/// `sys.argv[1]` 是内核根目录（`python -c <code> <root>`）。
const CONSOLE_WRAPPER: &str = r#"
import asyncio, sys
from pathlib import Path
from aurora.utils.environment import load_project_env
from aurora.configuration import load_config
from aurora.runtime import run_project

def readline(prompt):
    # 不回显提示：输入由界面负责显示，避免和输出行黏在一起
    line = sys.stdin.readline()
    if line == "": raise EOFError
    return line.rstrip("\n")

def output(text):
    # \x01 标记这是"终端输出"（对话），\x02 代替换行，保证一次 output 调用 = 一条消息；
    # 内核其它直接 print 到 stdout 的内容（如 Panel/Token 提示）不带标记，不会进对话。
    sys.stdout.write("\x01" + str(text).replace("\n", "\x02") + "\n")
    sys.stdout.flush()

def main():
    root = Path(sys.argv[1])
    load_project_env(root)
    asyncio.run(run_project(load_config(root), headless=False, readline=readline, output=output))

main()
"#;

#[derive(Clone)]
pub struct BotService {
    pub paths: std::sync::Arc<RuntimePaths>,
    pub settings: std::sync::Arc<SettingsStore>,
    pub bot: std::sync::Arc<BotRegistry>,
    /// 与安装命令共用的互斥标志：启动前的 setup 也要占住它，
    /// 否则手动安装与自动 setup 可能同时改同一个工具目录。
    pub installing: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl BotService {
    pub fn new(state: &AppState) -> Self {
        Self {
            paths: state.paths.clone(),
            settings: state.settings.clone(),
            bot: state.bot.clone(),
            installing: state.installing.clone(),
        }
    }

    /// 占住安装互斥，返回的守卫 drop 时释放（含提前 return / 出错路径）。
    fn acquire_install(&self) -> Result<InstallGuard> {
        self.installing
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .map_err(|_| anyhow::anyhow!("已有安装任务进行中，请等待当前任务完成"))?;
        Ok(InstallGuard {
            flag: self.installing.clone(),
        })
    }

    /// 初始化内核：确保工具 → 克隆内核（含子模块）→ 拷 `config`/`.env` → 建 venv → `uv sync`。
    /// 等价内核的 `aurora setup`，但**跳过 docs/panel 两个前端的 pnpm 依赖**（跑 Bot 用不到）。
    /// `rebuild_stale_venv`：venv 的来源与当前选择不一致时是否重建。
    /// 启动路径传 false（重建是重操作，不该让启动莫名变慢），「初始化」传 true。
    pub async fn setup(&self, app: &AppHandle, rebuild_stale_venv: bool) -> Result<()> {
        events::log(
            app,
            "info",
            "运行启动器 setup（等价 aurora setup，跳过 docs/panel）",
        );

        let tools = ToolService {
            paths: self.paths.clone(),
            settings: self.settings.clone(),
            bot: self.bot.clone(),
        };
        // 依次保证可用：launcher 自管的没装时，直接复用系统已装版本，都不存在才联网下载
        for kind in [ToolKind::Git, ToolKind::Uv, ToolKind::Python] {
            if !tools.is_installed(kind).await {
                tools.install(kind, app, false).await?;
            }
        }

        let uv = tools
            .uv_command_exe()
            .ok_or_else(|| anyhow::anyhow!("未找到可用的 uv"))?;

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

        // venv 只建一次，切换 Python 来源不会重建它，于是出现「设置说 A、实际跑 B」。
        // 是否重建由调用方决定：启动不重建（避免启动莫名变慢），「初始化」才重建。
        let desired_source = crate::tools::python_source(&tools).await;
        let venv_actual = crate::tools::venv_source(&tools);
        let stale = rebuild_stale_venv
            && self.paths.venv_ready()
            && venv_actual
                .as_deref()
                .is_some_and(|recorded| recorded != desired_source);

        if !self.paths.venv_ready() || stale {
            if stale {
                events::log(
                    app,
                    "warn",
                    format!(
                        "虚拟环境是用「{}」创建的，当前选择的是「{}」，正在按当前来源重建",
                        source_label(venv_actual.as_deref().unwrap_or("")),
                        source_label(desired_source)
                    ),
                );
            }
            // 创建中断会留下缺少 pyvenv.cfg 的残壳，必须先清掉再重建，否则 uv 会报
            // “No pyvenv.cfg file” 而无法同步依赖。
            if self.paths.env_aurora.exists() {
                if !stale {
                    events::log(
                        app,
                        "warn",
                        "检测到不完整的 Python 虚拟环境，正在清理并重建",
                    );
                }
                // 先改名再删除：目录被占用时改名会直接失败，这样不会留下删了一半的残壳。
                let broken = self.paths.env_aurora.with_extension("broken");
                let _ = std::fs::remove_dir_all(&broken);
                std::fs::rename(&self.paths.env_aurora, &broken).with_context(|| {
                    format!(
                        "无法重建 Python 虚拟环境：{} 正在被占用（可能还有 AuroraBot 在运行），请先停止 AuroraBot 后重试",
                        self.paths.env_aurora.display()
                    )
                })?;
                let _ = std::fs::remove_dir_all(&broken);
            }
            events::log(app, "info", "创建 AuroraBot Python 虚拟环境");
            let mut venv = Sandbox::new(&self.paths, &self.settings).command(&uv);
            venv.arg("venv").arg("--python");
            // 默认自管 Python 优先；设置里选了“系统版本”则用系统解释器
            let system_python = tools.system_python();
            let prefer_system = self.settings.tool_prefers_system(ToolKind::Python);
            // 选了“系统版本”且系统解释器存在时才用它，否则一律走自管 Python
            let preferred_system = if prefer_system {
                system_python.as_ref()
            } else {
                None
            };
            if let Some(python) = preferred_system {
                venv.arg(python);
            } else if tools.managed_python_ready().await {
                // 自管 Python：交给 uv 按其版本解析
                venv.arg(crate::manifest::PYTHON_VERSION);
            } else {
                // 复用系统 Python：显式传解释器路径
                let python = system_python.ok_or_else(|| anyhow::anyhow!("未找到可用的 Python"))?;
                venv.arg(&python);
            }
            venv.arg(&self.paths.env_aurora);
            let output = capture_output(venv, "uv venv").await?;
            ensure_success(output, "uv venv")?;
            // 记录来源，供下次启动判断是否需要按新来源重建
            self.settings
                .update(|settings| settings.venv_source = desired_source.to_string())?;
        }

        events::log(app, "info", "同步 AuroraBot Python 依赖");
        events::progress(app, "sync", 0, None, Some("同步 Python 依赖".into()));
        let mut sync = Sandbox::new(&self.paths, &self.settings).command(&uv);
        sync.arg("sync").arg("--active").arg("--project").arg(root);
        let output = capture_sync(sync, app.clone()).await?;
        if let Err(error) = ensure_success(output, "uv sync") {
            let detail = format!("{error:#}");
            bail!("{detail}\n{}", sync_failure_hint(&detail));
        }
        events::progress(app, "sync", 0, None, None);
        events::log(app, "success", "Python 依赖已同步");
        Ok(())
    }

    /// 启动前准备：初始化内核 + 配置体检（只提示，不阻塞启动）。
    pub async fn prepare(&self, app: &AppHandle) -> Result<()> {
        // 启动路径：不因来源不一致重建 venv，只提示（界面显示 pythonVenvStale）
        self.setup(app, false).await?;
        // 配置只是从模板复制来的：如果没填密钥/没启用平台，Bot 起来也连不上任何东西。
        for warning in config_warnings(&self.paths.kernel_aurora) {
            events::log(app, "warn", warning);
        }
        Ok(())
    }

    pub async fn start(&self, app: &AppHandle) -> Result<AuroraProcessInfo> {
        if self.bot.info().is_some() {
            bail!("AuroraBot 已在运行");
        }
        // 启动前会跑 setup（可能安装工具、同步依赖），期间占住安装互斥，
        // 避免与「安装依赖」命令同时写同一个工具目录。
        let install_guard = self.acquire_install()?;
        let prepared = self.prepare(app).await;
        drop(install_guard);
        prepared?;

        let kernel = &self.paths.kernel_aurora;
        if !self.paths.venv_ready() {
            bail!("虚拟环境不可用：{}", self.paths.env_python().display());
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

        // 用注入 readline/output 的包装脚本启动内核本地终端：内核默认走
        // prompt_toolkit（需要 TTY），在管道下会报错；换成逐行读 stdin 后，
        // 「对话」页就能把输入写进 stdin、把 Bot 输出显示出来。
        let mut cmd = Sandbox::new(&self.paths, &self.settings).command(&self.paths.env_python());
        cmd.current_dir(kernel);
        cmd.arg("-c").arg(CONSOLE_WRAPPER).arg(kernel);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        let mut child = cmd.spawn().context("启动 AuroraBot 失败")?;
        let pid = child.id();
        let started_at = Local::now().to_rfc3339();
        let stdin = child.stdin.take();

        // Bot 的 stdout/stderr 同时写日志文件并实时推到前端「运行日志」；
        // stdout（对话）额外发 bot-output 给「对话」页。
        if let Some(stdout) = child.stdout.take() {
            stream_bot_output(stdout, output_file.try_clone()?, app.clone(), true);
        }
        if let Some(stderr) = child.stderr.take() {
            stream_bot_output(stderr, output_file, app.clone(), false);
        }

        self.bot.set(BotRecord {
            child,
            pid,
            started_at: started_at.clone(),
            log_file: log_file.clone(),
            stdin: std::sync::Mutex::new(stdin),
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

    /// 往 Bot 的 stdin 写入一行对话输入。
    pub fn send_input(&self, text: &str) -> Result<()> {
        self.bot.send_input(text)
    }
}

/// 安装互斥守卫：drop 时释放 `installing`，任何返回路径（含 `?` 提前返回）都会释放。
struct InstallGuard {
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        self.flag
            .store(false, std::sync::atomic::Ordering::Release);
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

/// 把 venv 来源标识（`managed` / `system`）转成界面与日志用的中文。
fn source_label(source: &str) -> &str {
    match source {
        "managed" => "启动器副本",
        "system" => "系统版本",
        other => other,
    }
}

/// 启动前的配置体检：`.env` 是否有密钥、`config/apps.toml` 是否启用了平台。
/// 只返回提示信息，不阻塞启动。
fn config_warnings(root: &Path) -> Vec<String> {
    let mut warnings = Vec::new();

    let env_text = std::fs::read_to_string(root.join(".env")).unwrap_or_default();
    let has_secret = env_text.lines().any(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return false;
        }
        match line.split_once('=') {
            Some((name, value)) => {
                let name = name.trim();
                (name.ends_with("_API_KEY") || name.ends_with("_TOKEN")) && !value.trim().is_empty()
            }
            None => false,
        }
    });
    if !has_secret {
        warnings.push(
            "未在 .env 配置任何密钥（如 DEEPSEEK_API_KEY），Bot 无法调用模型；请填写 .env 后重启"
                .into(),
        );
    }

    let apps_text =
        std::fs::read_to_string(root.join("config").join("apps.toml")).unwrap_or_default();
    let has_enabled_app = apps_text.lines().any(|line| {
        let line = line.split('#').next().unwrap_or("").trim();
        line.starts_with("enabled") && line.contains("true")
    });
    if !has_enabled_app {
        warnings.push(
            "未在 config/apps.toml 启用任何平台（app），Bot 不会连接任何消息平台；请启用后重启"
                .into(),
        );
    }

    warnings
}

/// 把 Bot 子进程的输出逐行写日志文件，同时作为日志事件实时推给前端。
/// `is_stdout` 为真时（对话走 stdout、内核日志走 stderr）额外发 `bot-output` 给「对话」页。
fn stream_bot_output<R: Read + Send + 'static>(
    reader: R,
    mut file: File,
    app: AppHandle,
    is_stdout: bool,
) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut line = Vec::new();
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let _ = file.write_all(&line);
                    let _ = file.flush();
                    let raw = String::from_utf8_lossy(&line);
                    // 带 \x01 标记的才是终端输出（对话）；其它 stdout/stderr 只进日志
                    let chat = if is_stdout && raw.starts_with('\u{1}') {
                        Some(raw[1..].replace('\u{2}', "\n"))
                    } else {
                        None
                    };
                    let text = strip_ansi(chat.as_deref().unwrap_or(&raw));
                    let text = text.trim();
                    if !text.is_empty() {
                        events::log(&app, bot_log_level(text), text.to_string());
                        if chat.is_some() {
                            events::bot_output(&app, text);
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });
}

/// 粗判 Bot 日志级别，让前端“运行日志”按颜色区分。
/// 只认“独立成词”的级别标记（如 `| ERROR |`、`[WARNING]`、`Traceback (most recent...`），
/// 避免把正文里的 `error_handler`、`warn_once` 这类标识符误判成错误/警告行。
fn bot_log_level(line: &str) -> &'static str {
    let mut level = "info";
    for token in line.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        match token.to_ascii_uppercase().as_str() {
            "ERROR" | "CRITICAL" | "FATAL" | "EXCEPTION" | "TRACEBACK" => return "error",
            "WARN" | "WARNING" => level = "warn",
            _ => {}
        }
    }
    level
}

/// 去掉 ANSI 转义序列（彩色/光标控制），避免日志里出现乱码控制符。
/// 覆盖 CSI（`ESC [ ... m/K/H`）与 OSC（`ESC ] ... BEL` 或 `ESC \`，改标题/超链接），
/// 其余双字符转义（如 `ESC ( B`）整体丢弃。
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\u{1b}' => match chars.next() {
                // CSI：参数以 0x40-0x7E 的终止字节结束
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&c) {
                            break;
                        }
                    }
                }
                // OSC：以 BEL 或 ST（ESC \）结束
                Some(']') => {
                    let mut after_esc = false;
                    for c in chars.by_ref() {
                        if c == '\u{7}' || (after_esc && c == '\\') {
                            break;
                        }
                        after_esc = c == '\u{1b}';
                    }
                }
                Some(_) => {}
                None => break,
            },
            '\r' => {}
            _ => out.push(ch),
        }
    }
    out
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

/// 按 `uv sync` 的失败内容给出针对性提示：只有网络/下载类错误才提下载源，
/// 其余（临时目录缺失、依赖冲突等）给对应或通用提示，避免一律甩锅“下载源”。
fn sync_failure_hint(detail: &str) -> &'static str {
    let lower = detail.to_ascii_lowercase();
    // 临时目录缺失/不可写（os error 3）：启动器每次跑子进程前都会自愈，这里给可操作的兜底
    if detail.contains("os error 3") || detail.contains("系统找不到指定的路径") {
        return "提示：运行目录下的临时文件夹缺失或不可写，请重启启动器后重试；若仍失败，可删除运行目录下的 tool 文件夹后重新初始化。";
    }
    if [
        "failed to fetch",
        "failed to download",
        "request failed",
        "timed out",
        "timeout",
        "error sending request",
        "connection",
        "dns",
        "temporary failure in name resolution",
        "ssl",
        "tls",
        "certificate",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        return "提示：下载依赖失败，可能是网络或包下载源问题，可在「设置 → 下载源」切换（国内镜像 / 官方源），或检查网络/代理后重试。";
    }
    "提示：请查看「运行日志」了解详细错误；常见原因是网络不通或依赖安装被中断，可重试或重新初始化。"
}

async fn capture_output(mut cmd: Command, label: &'static str) -> Result<Output> {
    tokio::task::spawn_blocking(move || cmd.output().with_context(|| format!("执行 {label} 失败")))
        .await
        .with_context(|| format!("{label} 任务失败"))?
}

/// uv sync 阶段 → 近似百分比（uv 不输出字节级进度，按输出里的里程碑映射）。
/// 解析→18%，下载→55%，安装/审计→90%；结束前会切到 100%。
fn sync_stage_percent(line: &str) -> Option<u8> {
    let l = line.to_ascii_lowercase();
    if l.contains("installed") || l.contains("audited") {
        Some(90)
    } else if l.contains("downloaded") {
        Some(55)
    } else if l.contains("resolved") {
        Some(18)
    } else {
        None
    }
}

/// 流式执行 uv sync：边读 stderr 边按阶段更新进度，让前端有带百分比的进度条
async fn capture_sync(mut cmd: Command, app: AppHandle) -> Result<Output> {
    tokio::task::spawn_blocking(move || {
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        let mut child = cmd.spawn().with_context(|| "启动 uv sync 失败")?;
        let stderr = child.stderr.take().context("读取 uv stderr 失败")?;
        let mut stdout = child.stdout.take().context("读取 uv stdout 失败")?;

        let app_thread = app.clone();
        let err_handle = std::thread::spawn(move || -> Vec<u8> {
            let mut bytes = Vec::new();
            let mut line = String::new();
            let mut reader = BufReader::new(stderr);
            let mut last: u8 = 0;
            events::progress(
                &app_thread,
                "sync",
                5,
                Some(100),
                Some("同步 Python 依赖".into()),
            );
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        bytes.extend_from_slice(line.as_bytes());
                        if let Some(pct) = sync_stage_percent(&line) {
                            if pct > last {
                                last = pct;
                                events::progress(
                                    &app_thread,
                                    "sync",
                                    u64::from(pct),
                                    Some(100),
                                    Some("同步 Python 依赖".into()),
                                );
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            bytes
        });

        let out_handle = std::thread::spawn(move || -> Vec<u8> {
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

        let status = child.wait().context("等待 uv sync 结束失败")?;
        let stderr_bytes = err_handle.join().unwrap_or_default();
        let stdout_bytes = out_handle.join().unwrap_or_default();
        Ok(Output {
            status,
            stdout: stdout_bytes,
            stderr: stderr_bytes,
        })
    })
    .await
    .with_context(|| "uv sync 任务失败")?
}

