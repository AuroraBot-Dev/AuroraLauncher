# AuroraLauncher

AuroraLauncher 是 AuroraBot 的 Tauri 桌面启动器。它只做三件事：把运行工具下载到自己的目录、把 AuroraBot 内核作为 Git 仓库更新、以受控方式启动 Bot。

## 目录结构

```text
launcher/
├── package.json
├── pnpm-lock.yaml
├── pnpm-workspace.yaml
├── src/                        Vue UI（Naive UI）
│   ├── main.ts
│   ├── App.vue
│   ├── stores/app.ts           IPC 状态与事件
│   ├── views/                  Dashboard / Wizard / About
│   └── types.ts
├── src-tauri/                  Rust 后端
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/           最小窗口权限
│   └── src/
│       ├── main.rs             应用装配与命令注册
│       ├── commands.rs         暴露给 Vue 的命令
│       ├── state.rs            AppState、运行时路径、设置、Bot 进程
│       ├── manifest.rs         工具版本与下载包描述
│       ├── platform.rs         平台探测
│       ├── sandbox.rs          受控子进程环境
│       ├── download.rs         断点下载与校验
│       ├── archive.rs          解压
│       ├── tools.rs            工具安装、自检、版本状态
│       ├── kernel.rs           Git 克隆、更新、状态
│       ├── bot.rs              内核准备、uv 依赖同步、Bot 生命周期
│       └── events.rs           进度与日志事件
└── README.md
```

## 架构

```text
Vue UI（薄前端）
   │  invoke / event
Tauri IPC 命令层
   │
Rust AppState（唯一状态）
   ├── ToolService    下载与管理 Python/uv/Git/pnpm
   ├── KernelService  下载与 Git 更新 AuroraBot
   ├── BotService     创建 venv、同步依赖、启动/停止 Bot
   └── SandboxRunner  每次启动子进程时重写干净环境
```

设计约束：

- Vue 不做文件系统、进程或网络操作。
- 所有关键任务都经过 Rust AppState。
- 所有下载先进入 `downloads/`，完成校验和解压 staging 后才进入正式目录。
- 所有子进程都使用 SandboxRunner，宿主 Python、Git、pnpm 不会被读取。
- Bot 启动前必须完成内核 clone、个人配置复制和 `uv sync`。
- 更新内核前会检查本地修改，拒绝覆盖被用户改动过的 tracked 文件。

## 运行时目录

采用便携模式：整套运行数据放在可执行文件同级目录 `tool/` 下，与 `aurora-launcher.exe` 一起整体拷贝即可迁移：

```text
<exe 所在目录>/
├── aurora-launcher.exe
└── tool/                  运行根目录（可整体删除以重置）
    ├── state/             settings.json 与安装记录
    ├── downloads/         下载临时缓存
    ├── staging/           未完成安装的 staging 目录
    ├── tools/python/      uv 管理的 Python
    ├── tools/uv/          uv
    ├── tools/git/         MinGit
    ├── tools/pnpm/        pnpm standalone
    ├── env/aurora/        AuroraBot 独立 venv
    ├── kernel/auroraBot/  AuroraBot Git 仓库
    ├── home/              受管 HOME 与用户配置
    ├── cache/             uv/pip/npm 缓存
    └── logs/              Bot 与 launcher 日志
```

删除整个 `tool/` 即完成运行时清理，launcher 不会修改宿主 PATH、注册表或全局 Python 环境。

## 工具管理

当前锁定版本定义在 `src-tauri/src/manifest.rs`：

- Python：由受管 uv 下载 `python-build-standalone`，版本 `3.12.7`
- uv：官方 GitHub release
- Git：Windows 使用官方 MinGit；macOS/Linux 的便携 Git 资产需要发布流水线注入同一 manifest
- pnpm：官方 standalone 可执行文件，不要求先安装 Node

工具安装流程：

```text
读取 manifest → 下载 .part → SHA256（若提供）→ staging → 自检 --version
→ 原子改名 → 更新 settings.json → 通知 Vue
```

工具安装目录按 `tools/<tool>/` 组织；重新安装时先切到 staging，再切回正式目录，避免留下半成品。

工具就绪判定优先 launcher 自管目录：若自管未装、但系统 PATH 中存在可用版本（git/uv/python/pnpm），则直接复用并显示为“就绪”，不再重复下载；只有两处都没有时才联网下载到 `tool/tools/`。

## 环境隔离

SandboxRunner 每次执行外部命令都会：

1. `env_clear()` 清空宿主环境。
2. PATH 只放 launcher 管理的工具目录和必要系统目录。
3. HOME/USERPROFILE/APPDATA/TEMP 指向 `runtime/home`。
4. 设置 `UV_CACHE_DIR`、`UV_PYTHON_INSTALL_DIR`、`UV_PROJECT_ENVIRONMENT`、`PIP_CACHE_DIR`、`NPM_CONFIG_CACHE`、`GIT_CONFIG_NOSYSTEM`。
5. 删除宿主 `VIRTUAL_ENV`、`CONDA_*`、`PYTHONHOME` 等变量。

因此，AuroraBot 看到的 Git 是 launcher 管理的 Git，Python 是 launcher 管理的 Python，缓存也只写在 launcher 的 runtime 中。

## 内核准备

`kernel_update` 会执行：

```text
检查 Bot 是否运行 → 检查本地 tracked 修改 → git fetch
→ reset --hard FETCH_HEAD → 同步 docs/panel 子模块 → 保存 commit
```

`start_bot` 会先执行准备：

```text
安装缺失工具 → clone/更新内核 → 从 config.example 复制 config/
→ 从 .env.example 复制 .env → 创建 runtime/env/aurora
→ uv sync --project kernel → 启动 aurora.main
```

`.env`、`config/`、`data/`、`logs/` 都在内核仓库的 `.gitignore` 范围内，Git 更新不会删除它们。

## 开发

前置要求：

- Node.js 20+
- pnpm 10+
- Rust stable
- Windows WebView2

安装依赖：

```bash
pnpm install
```

启动前端开发：

```bash
pnpm dev
```

启动 Tauri 开发窗口：

```bash
pnpm tauri:dev
```

构建前端：

```bash
pnpm build
```

构建桌面安装包：

```bash
pnpm tauri:build
```

只检查 Rust：

```bash
cd src-tauri
cargo check
```

## 自动更新与发布

应用使用 Tauri 官方 updater（`tauri-plugin-updater`）：启动时会静默自动检查一次，
发现新版本即弹窗提示；底栏“检查更新”按钮可随时手动检查。查询地址是
`src-tauri/tauri.conf.json` 中配置的 `latest.json`（指向本仓库 GitHub Release）。
流程由 `.github/workflows/ci.yml`（PR/主分支校验）与 `.github/workflows/release.yml`
（推送 `v*` tag 时构建三平台并发布）驱动。

### 首次配置签名密钥

更新包必须签名校验，首次发布前需要一次性生成密钥：

```bash
pnpm tauri signer generate -w ~/.tauri/aurora-launcher.key
```

1. 把生成的公钥粘贴进 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`
   （当前是占位符 `REPLACE_WITH_YOUR_PUBLIC_KEY`）。
2. 在 GitHub 仓库 `Settings → Secrets and variables → Actions` 添加两个密钥：
   - `TAURI_SIGNING_PRIVATE_KEY`：`~/.tauri/aurora-launcher.key` 文件的完整内容
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：口令（如果生成时设置了密码）

公钥会内嵌到安装包；私钥只存在于 GitHub Secrets，请勿提交到仓库，丢失后无法再向老用户推送更新。

### 发布流程

版本单一来源是根目录 `package.json` 的 `version`：
`src-tauri/tauri.conf.json` 用 `"version": "../package.json"` 引用它，
`src-tauri/Cargo.toml` 里的版本仅作 Rust crate 元数据、不再决定打包版本。

1. 只改 `package.json` 的版本号（也可 `pnpm version 0.2.0`）。
2. 打与版本一致的 tag 并推送：

   ```bash
   git add package.json
   git commit -m "chore: bump version to 0.2.0"
   git push origin main

   git tag v0.2.0
   git push origin v0.2.0
   ```

3. `.github/workflows/release.yml` 会自动为 Windows（NSIS/MSI）、macOS Apple Silicon（dmg）、
   Linux（deb/rpm/AppImage，以及一个便携 `tar.gz`）构建产物，
   生成 updater 签名与 `latest.json`，并把资产上传到 draft Release；
   确认无误后在 GitHub 页面点“发布”。
4. 用户端底栏“检查更新”→“立即更新”即可自动升级。

注意：因为开启了 `bundle.createUpdaterArtifacts`，本地执行 `pnpm tauri:build`
也会要求环境变量 `TAURI_SIGNING_PRIVATE_KEY`（PowerShell：`$env:TAURI_SIGNING_PRIVATE_KEY="..."`）。
如果只是验证能否编译（不打更新产物），用 `cargo check` 即可。
macOS 未配置 Apple 开发者证书时产物不做公证/签名，仅用于内部测试与 updater 验证；
如需正式对外分发请按 Tauri 文档补充 `APPLE_*` 证书与 notarization 配置。

## IPC 命令

| 前端调用 | Rust 命令 | 作用 |
| --- | --- | --- |
| `check_all_status` | `check_all_status` | 返回依赖、内核、Bot 状态 |
| `install_all_deps` | `install_all_deps` | 按 Git、uv、Python、pnpm 顺序安装 |
| `install_dependency` | `install_dependency` | 安装单个工具 |
| `kernel_update` | `kernel_update` | 下载或 Git 更新内核 |
| `start_bot` | `start_bot` | 准备环境并启动 Bot |
| `stop_bot` | `stop_bot` | 停止当前 Bot |
| `open_app_dir` | `open_app_dir` | 打开运行时目录 |
| `checkForUpdates` | `check_launcher_update` | 检查 AuroraLauncher 自身更新 |
| `installUpdate` | `install_launcher_update` | 下载并安装检测到的更新 |

后端事件：

- `progress`：`{ kind, current, total?, label? }`
- `log`：`{ level, message }`
- `bot-state-changed`：`{ running, pid, startedAt, logFile }`

## 常见问题

- 首次安装失败：先确认网络可达 GitHub；失败时日志会显示具体 URL。
- 内核目录被手动修改：更新会拒绝继续，避免覆盖本地工作。
- Bot 启动失败：查看 `runtime/logs/aurora-*.log`。
- macOS/Linux 无法安装 Git：当前发布包需在 manifest 中补充对应平台的便携 Git 二进制；Windows 已内置官方 MinGit。
