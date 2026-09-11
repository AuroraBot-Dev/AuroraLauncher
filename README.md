# AuroraLauncher

中文 | [English](README.en.md) | [日本語](README.ja.md)

AuroraBot 的 Tauri 桌面启动器。它把运行工具（Python / uv / Git / pnpm）下载到自己的目录、把 AuroraBot 内核作为 Git 仓库更新，并以受控方式启动 Bot；全程不修改宿主 PATH、注册表或全局 Python 环境。

界面是 Vue 3（Naive UI）薄前端，所有文件系统 / 进程 / 网络操作都在 Rust 后端，经 Tauri IPC 通信。

## 前置

- Node.js 20+
- pnpm 10+
- Rust stable
- Windows 需 WebView2 运行时

## 本地开发

安装依赖：

```bash
pnpm install
```

浏览器里预览界面。此模式为**演示模式**：界面数据由前端模拟，安装 / 启动不会调用后端，也不会改动本机。

```bash
pnpm dev
```

启动真实 Tauri 开发窗口（会先自动执行 `pnpm dev`，devUrl 固定 `http://localhost:1420`）：

```bash
pnpm tauri:dev
```

只做类型检查与前端构建：

```bash
pnpm build          # vue-tsc --noEmit && vite build
```

只检查 Rust 后端（比完整打包快，且不要求签名密钥）：

```bash
cd src-tauri
cargo check
```

## 运行时目录

便携模式：整套运行数据放在可执行文件同级 `tool/` 下，与 exe 一起整体拷贝即可迁移；删除整个 `tool/` 即完成重置。若 exe 所在目录不可写（例如 Linux 包管理器装到 `/usr/bin`），自动改用家目录下的 `~/.aurora-launcher/tool`（三平台路径一致），不会为写盘索取 root 权限。

```text
<exe 所在目录>/
├── aurora-launcher.exe
└── tool/                  运行根目录
    ├── state/             settings.json 与安装记录
    ├── downloads/         下载缓存
    ├── staging/           未完成安装的 staging
    ├── tools/             python / uv / git / pnpm
    ├── env/aurora/        AuroraBot 独立 venv
    ├── kernel/auroraBot/  AuroraBot Git 仓库
    ├── home/              受管 HOME 与用户配置
    ├── cache/             uv / pip / npm 缓存
    └── logs/              Bot 与 launcher 日志
```

## 架构

Vue 只负责展示与交互，Rust `AppState` 是唯一状态源：

```text
Vue UI ──invoke / event──► Tauri 命令层 ──► AppState
                                            ├── ToolService   下载并管理 Python/uv/Git/pnpm
                                            ├── KernelService 克隆 / 更新 AuroraBot 内核
                                            ├── BotService    建 venv、uv sync、启动 / 停止 Bot
                                            └── SandboxRunner 每次子进程都重写干净环境
```

关键约束：

- 安装固定走 `downloads/ → staging/ → 原子改名进 tools/<tool>/`，不直接写正式目录。
- 所有子进程经 SandboxRunner（`env_clear()`，PATH / HOME / 缓存全部指向 `tool/`）。
- 工具就绪优先自管目录；缺失时复用系统 PATH 上的可用版本，都没有才联网下载。
- 内核更新前检查本地 tracked 修改，拒绝覆盖用户改动。

工具版本锁定在 `src-tauri/src/manifest.rs`：Python `3.12.7`、uv `0.12.10`、Git `2.46.0`、pnpm `9.12.0`。

## 自动更新与发布

应用使用 Tauri 官方 updater（`tauri-plugin-updater`）：启动时静默检查一次，发现新版本弹窗提示；「关于 → 检查更新」可随时手动检查。查询地址是 `src-tauri/tauri.conf.json` 中指向本仓库 Release 的 `latest.json`。

版本单一来源是根目录 `package.json` 的 `version`（`tauri.conf.json` 用 `"version": "../package.json"` 引用它）：

```bash
pnpm version 0.2.0
git commit -am "chore: bump version to 0.2.0"
git push origin main
git tag v0.2.0 && git push origin v0.2.0
```

推送 `v*` tag 后，`.github/workflows/release.yml` 会为 Windows（NSIS/MSI）、macOS Apple Silicon、Linux（deb/rpm/AppImage 及便携 tar.gz）构建产物，生成 updater 签名与 `latest.json` 并上传到 draft Release，确认后点“发布”。

更新包必须签名校验，首次发布前一次性生成密钥：

```bash
pnpm tauri signer generate -w ~/.tauri/aurora-launcher.key
```

- 公钥粘贴进 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`。
- GitHub `Settings → Secrets and variables → Actions` 添加 `TAURI_SIGNING_PRIVATE_KEY`（私钥文件内容）与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。

私钥只存在于 GitHub Secrets，请勿提交；丢失后无法再向老用户推送更新。本地 `pnpm tauri:build` 因开启了 `createUpdaterArtifacts` 也会要求 `TAURI_SIGNING_PRIVATE_KEY`，只验证编译用 `cargo check` 即可。

## IPC 命令

| 前端调用 | Rust 命令 | 作用 |
| --- | --- | --- |
| `check_all_status` | `check_all_status` | 返回依赖、内核、Bot 状态 |
| `install_all_deps` | `install_all_deps` | 按 Git、uv、Python、pnpm 顺序安装 |
| `install_dependency` | `install_dependency` | 安装单个工具 |
| `setToolDir` | `set_tool_dir` | 设置 / 清除某工具的自定义安装目录 |
| `kernel_update` | `kernel_update` | 下载或 Git 更新内核 |
| `start_bot` | `start_bot` | 准备环境并启动 Bot |
| `stop_bot` | `stop_bot` | 停止当前 Bot |
| `open_app_dir` | `open_app_dir` | 打开运行时目录 |
| `open_external_url` | `open_external_url` | 用默认浏览器打开外链 |
| `runtime_info` | `runtime_info` | 返回系统、架构与运行时目录 |
| `checkForUpdates` | `check_launcher_update` | 检查自身更新 |
| `installUpdate` | `install_launcher_update` | 下载并安装检测到的更新 |

后端事件：

- `progress`：`{ kind, current, total?, label? }`
- `log`：`{ level, message }`
- `bot-state-changed`：`{ running, pid, startedAt, logFile }`

## 常见问题

- 首次安装失败：确认网络可达 GitHub，失败日志会显示具体 URL。
- 内核目录被手动修改：更新会拒绝继续，避免覆盖本地工作。
- Bot 启动失败：查看 `tool/logs/` 下的 Bot 日志。
- macOS / Linux 无法安装 Git：需在 `manifest.rs` 补充对应平台的便携 Git 二进制；Windows 已内置官方 MinGit。
