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

运行数据默认放在家目录下的 `~/.aurora-launcher/tool`（三平台路径一致），避免 exe 装在只读/系统目录（如 Linux 的 `/usr/bin`）时同级不可写。若 exe 同级**已经存在** `tool/`（便携版，随 exe 一起拷贝即迁移），则优先用它。删除整个 `tool/` 即完成重置，不会为写盘索取 root 权限。

顶层按用途分四块：`runtime/`（运行必需）、`data/`（用户数据）、`cache/`（可删缓存）、`logs/`。

```text
<exe 所在目录>/
├── aurora-launcher.exe
└── tool/                  运行根目录（顶层固定 4 个目录）
    ├── runtime/           运行必需
    │   ├── tools/         受管工具本体：git / uv / python / pnpm
    │   ├── venv/          AuroraBot 独立 venv（底座解释器 + 项目依赖）
    │   └── kernel/auroraBot/  AuroraBot Git 仓库（含 docs / panel 子模块）
    ├── data/              用户数据
    │   ├── home/          沙箱 HOME：AppData、temp、.gitconfig
    │   └── state/         settings.json 与安装记录
    ├── cache/             可删缓存（删掉会自动重建）
    │   ├── uv/ pip/ npm/   各工具的包缓存
    │   ├── downloads/      下载的安装包
    │   └── staging/        解压暂存（原子安装用）
    └── logs/              Bot 与 launcher 日志
```

说明：`runtime/tools/` 是工具**本体**（可执行文件），`cache/uv` 是 uv 的**包缓存**，二者不同；`runtime/venv` 是虚拟环境，其中的 `Scripts/python.exe` 只是指向底座解释器的入口，不是第二份 Python。用系统 Python 作底座时 `runtime/tools/python/` 为空。

启动器自己写的关键文件：

| 文件 | 作用 |
| --- | --- |
| `data/state/settings.json` | 工具状态、自定义安装目录、来源偏好、内核 remote/branch |
| `logs/launcher-status.log` | 每次状态探测的 JSON（排查"显示缺失"用） |
| `logs/aurora-*.log` | 每次启动 Bot 的 stdout/stderr（「运行日志」页也会实时显示） |
| `data/home/.gitconfig` | 沙箱 git 全局配置（默认关证书校验，保证 clone 可达） |
| `runtime/venv/pyvenv.cfg` | venv 标记，`home` 指向底座解释器 |
| `cache/downloads/*` | 下载的安装包（用于校验 / 断点续传） |

`runtime/tools/` 里是第三方工具本体（如 MinGit 的 `cmd/git.exe`、`mingw64/`），由安装流程解压而来，不是启动器源码。

## 代码结构

```text
src/                        前端（Vue 3 + Naive UI 薄壳，不含任何 fs/进程/网络操作）
├── main.ts                 入口：挂载、注册事件监听、首次刷新
├── App.vue                 全局布局：侧栏 + 路由视图 + 消息 / 对话框 Provider
├── router/index.ts         路由表（hash 模式）
├── stores/app.ts           唯一前端状态源，封装全部 invoke / listen
├── types.ts                与后端共享的数据结构
├── theme.ts                明暗主题的 Naive UI 覆盖（强调色统一为 #006be6）
├── format.ts               展示格式化（工具版本等）
├── links.ts                对外链接集中管理
├── views/                  每个路由一个页面
│   ├── Dashboard.vue       主页（内容为空，操作条在 MainActionBar）
│   ├── Chat.vue            对话：和 Bot 直接对话
│   ├── Environment.vue     运行环境：工具状态、安装 / 重装、来源切换
│   ├── Kernel.vue          内核：版本、更新、描述
│   ├── Config.vue          配置：密钥、消息平台开关
│   ├── Logs.vue            运行日志
│   ├── Settings.vue        外观、更新、运行时操作
│   └── About.vue           关于、社区、许可
└── components/
    ├── AppSidebar.vue      侧栏导航
    ├── MainActionBar.vue   底部主操作（下载核心 / 启动 / 停止）
    └── UpdateDialog.vue    自身更新弹窗

src-tauri/src/              后端（Rust，所有 fs / 进程 / 网络都在这里）
├── main.rs                 入口：注册命令、退出时停掉 Bot
├── commands.rs             #[tauri::command] 命令实现
├── state.rs                AppState / RuntimePaths / Settings / BotRegistry
├── tools.rs                工具安装、探测、来源解析、依赖状态
├── kernel.rs               内核克隆 / 更新 / 运行 `aurora about`
├── bot.rs                  准备环境、建 venv、启动 / 停止、转发 Bot 日志
├── config.rs               内核配置读写（.env 密钥、apps.toml 平台开关）
├── sandbox.rs              子进程统一沙箱（env_clear + PATH/HOME/缓存重定向）
├── download.rs             带校验 / 续传的下载
├── archive.rs              zip / tar 解压
├── hash.rs                 SHA256
├── manifest.rs             工具版本与下载地址（唯一版本来源）
├── events.rs               向前端发 log / progress 事件
├── updater.rs              启动器自身更新检查 / 安装
└── platform.rs             平台与路径工具
```

## 架构

Vue 只负责展示与交互，Rust `AppState` 是唯一状态源：

```text
Vue UI ──invoke / event──► Tauri 命令层 ──► AppState
                                            ├── ToolService   下载并管理 Python/uv/Git/pnpm
                                            ├── KernelService 克隆 / 更新 AuroraBot 内核
                                            ├── BotService    建 venv、uv sync、启动 / 停止 Bot
                                            ├── config        读写内核配置（.env / apps.toml）
                                            └── SandboxRunner 每次子进程都重写干净环境
```

关键约束：

- 安装固定走 `cache/downloads/ → cache/staging/ → 原子改名进 tools/<tool>/`，不直接写正式目录。
- 所有子进程经 SandboxRunner（`env_clear()`，PATH / HOME / 缓存全部指向 `tool/`）。
- 工具来源默认**自管优先**，缺失时复用系统 PATH 上的可用版本，都没有才联网下载；两份都存在时可在「运行环境 → 更多」里切换用哪一份（`set_tool_source`）。
- 版本与位置都以**实际探测**为准：界面显示的解释器 / 工具版本、目录都跟着当前使用的来源走。
- 内核 `uv sync` 的包源默认走国内 PyPI 镜像（沙箱注入 `UV_DEFAULT_INDEX`/`PIP_INDEX_URL`），可在「设置 → 下载源」切回官方；失败时错误信息会提示当前源。
- 工具安装包（Git / uv / pnpm）与 Python 解释器默认从 github.com 下载；可在「设置 → GitHub 加速」填加速前缀（工具 URL 下载前套前缀，沙箱注入 `UV_PYTHON_INSTALL_MIRROR`），留空 = 直连。下载器复用同一个 HTTP 客户端，带读超时、失败自动续传重试，并在底部进度条显示瞬时速度。
- 启动器自带 `setup`（`BotService::setup`）：确保 git/uv/python → 克隆内核（含子模块）→ 拷 `config`/`.env` → 建 venv → `uv sync`。它等价内核的 `aurora setup`，但**跳过 docs/panel 两个前端的 pnpm 依赖**（跑 Bot 用不到）。`prepare` = `setup` + 配置体检；「内核」页的「初始化」按钮可单独触发（`run_setup`）。
- 启动前会体检配置：`.env` 没有密钥、或 `apps.toml` 没启用任何平台时发警告（不阻塞启动）。
- Bot 的 stdout/stderr 同时写日志文件并实时转发到「运行日志」；stdout（对话）额外发 `bot-output` 给「对话」页。
- Bot 以「对话模式」启动：用注入 `readline`/`output` 的包装脚本替代内核默认的 prompt_toolkit 终端（后者需要 TTY，管道下会失败），把 stdin/stdout 接到「对话」页，实现应用内对话。
- 内核更新前检查本地 tracked 修改，拒绝覆盖用户改动。

工具版本锁定在 `src-tauri/src/manifest.rs`：Python `3.12.7`、uv `0.12.10`、Git `2.55.0.5`、pnpm `9.12.0`。

各平台安装包的 SHA256 也记在 `manifest.rs::sha256_of`，下载（含**已存在的缓存**）时强制校验，用来挡住被篡改的镜像 / GitHub 加速前缀。uv（六个平台）与 MinGit（Windows x64 / arm64）齐全，来源是 GitHub Release 资产的 `digest` 字段（已与官方 `.sha256` 抽查核对一致）。只有 pnpm `v9.12.0` 仍为 `None`（不校验）——该 release 上传得比 GitHub 的 digest 功能早，资产既无 `digest` 也无 `.sha256`；补齐方式：下载对应资产后 `Get-FileHash -Algorithm SHA256`（或 `sha256sum`），把值填进 `sha256_of` 即可。

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
| `install_dependency` | `install_dependency` | 安装单个工具 |
| `setToolDir` | `set_tool_dir` | 设置 / 清除某工具的自定义安装目录 |
| `setToolSource` | `set_tool_source` | 选择工具来源（启动器副本 / 系统版本） |
| `setDownloadSource` | `set_download_source` | 切换包下载源（国内镜像 / 官方源） |
| `setGithubMirror` | `set_github_mirror` | 设置 GitHub 加速前缀（留空 = 直连） |
| `read_launcher_config` | `read_launcher_config` | 读取内核配置：`.env` 变量与 `apps.toml` 平台 |
| `set_env_value` | `set_env_value` | 写入 `.env` 里的一个变量 |
| `set_app_enabled` | `set_app_enabled` | 开关 `config/apps.toml` 里的平台 |
| `runSetup` | `run_setup` | 手动运行启动器 setup（初始化内核） |
| `kernel_update` | `kernel_update` | 下载或 Git 更新内核 |
| `startBot` | `start_bot` | 准备环境并启动 Bot（对话模式，可交互） |
| `sendBotInput` | `send_bot_input` | 往 Bot 的 stdin 写一行对话输入 |
| `stop_bot` | `stop_bot` | 停止当前 Bot |
| `open_app_dir` | `open_app_dir` | 打开运行时目录 |
| `open_tool_dir` | `open_tool_dir` | 在文件管理器中定位某工具实际安装目录 |
| `open_kernel_dir` | `open_kernel_dir` | 在文件管理器中定位 AuroraBot 内核目录 |
| `kernel_about` | `kernel_about` | 运行内核 `aurora about` 获取内核描述 |
| `open_external_url` | `open_external_url` | 用默认浏览器打开外链 |
| `runtime_info` | `runtime_info` | 返回系统、架构与运行时目录 |
| `checkForUpdates` | `check_launcher_update` | 检查自身更新 |
| `installUpdate` | `install_launcher_update` | 下载并安装检测到的更新 |

后端事件：

- `progress`：`{ kind, current, total?, label? }`
- `log`：`{ level, message }`
- `bot-output`：Bot stdout 的原始行（对话，内核日志走 stderr 不发这个）
- `bot-state-changed`：`{ running, pid, startedAt, logFile }`

## 常见问题

- 首次安装失败：确认网络可达 GitHub，失败日志会显示具体 URL。
- 内核目录被手动修改：更新会拒绝继续，避免覆盖本地工作。
- Bot 启动失败：查看 `tool/logs/` 下的 Bot 日志。
- macOS / Linux 无法安装 Git：需在 `manifest.rs` 补充对应平台的便携 Git 二进制；Windows 已内置官方 MinGit。
