# AuroraLauncher

[中文](README.md) | English | [日本語](README.ja.md)

AuroraBot's Tauri desktop launcher. It downloads the runtime tools (Python / uv / Git / pnpm) into its own directory, updates the AuroraBot kernel as a Git repository, and launches the Bot in a controlled way. It never modifies the host PATH, registry, or global Python environment.

The UI is a thin Vue 3 (Naive UI) frontend; all filesystem / process / network operations live in the Rust backend and go through Tauri IPC.

## Prerequisites

- Node.js 20+
- pnpm 10+
- Rust stable
- Windows requires the WebView2 runtime

## Local development

Install dependencies:

```bash
pnpm install
```

Preview the UI in a browser. This runs in **demo mode**: the UI data is simulated in the frontend, installs / launches do not call the backend and nothing on your machine is changed.

```bash
pnpm dev
```

Start a real Tauri dev window (it runs `pnpm dev` first; devUrl is fixed to `http://localhost:1420`):

```bash
pnpm tauri:dev
```

Type-check and build the frontend only:

```bash
pnpm build          # vue-tsc --noEmit && vite build
```

Check the Rust backend only (faster than a full package, and no signing key required):

```bash
cd src-tauri
cargo check
```

## Runtime directory

Portable mode: all runtime data lives in a `tool/` folder next to the executable. Copy the executable together with `tool/` to migrate; delete the whole `tool/` folder to reset. If that directory is not writable (e.g. a Linux package installed the binary under `/usr/bin`), the launcher falls back to a per-user data directory (Linux `~/.local/share/AuroraLauncher/tool`, macOS `~/Library/Application Support/dev.AuroraBot.AuroraLauncher/tool`, Windows `%APPDATA%\AuroraBot\AuroraLauncher\data\tool`) and never asks for root just to write data.

```text
<exe dir>/
├── aurora-launcher.exe
└── tool/                  runtime root
    ├── state/             settings.json and install records
    ├── downloads/         download cache
    ├── staging/           staging for unfinished installs
    ├── tools/             python / uv / git / pnpm
    ├── env/aurora/        dedicated AuroraBot venv
    ├── kernel/auroraBot/  AuroraBot Git repository
    ├── home/              managed HOME and user config
    ├── cache/             uv / pip / npm caches
    └── logs/              Bot and launcher logs
```

## Architecture

Vue only handles presentation and interaction; the Rust `AppState` is the single source of truth:

```text
Vue UI ──invoke / event──► Tauri command layer ──► AppState
                                                   ├── ToolService   download & manage Python/uv/Git/pnpm
                                                   ├── KernelService clone / update the AuroraBot kernel
                                                   ├── BotService    create venv, uv sync, start / stop Bot
                                                   └── SandboxRunner rewrite a clean env for every child process
```

Key constraints:

- Installs always go `downloads/ → staging/ → atomic rename into tools/<tool>/`, never straight into the live directory.
- Every child process goes through SandboxRunner (`env_clear()`; PATH / HOME / caches all point into `tool/`).
- Tool readiness prefers the managed directory; if missing, a usable version on the host PATH is reused; only when neither exists does it download.
- Before updating the kernel it checks for local tracked changes and refuses to overwrite the user's work.

Pinned tool versions live in `src-tauri/src/manifest.rs`: Python `3.12.7`, uv `0.12.10`, Git `2.46.0`, pnpm `9.12.0`.

## Auto-update and release

The app uses the official Tauri updater (`tauri-plugin-updater`): it checks silently once at startup and shows a dialog when a new version is found; "About → Check for updates" checks on demand. The endpoint is the `latest.json` in this repository's Release, configured in `src-tauri/tauri.conf.json`.

The single source of the version is `version` in the root `package.json` (`tauri.conf.json` references it via `"version": "../package.json"`):

```bash
pnpm version 0.2.0
git commit -am "chore: bump version to 0.2.0"
git push origin main
git tag v0.2.0 && git push origin v0.2.0
```

After a `v*` tag is pushed, `.github/workflows/release.yml` builds artifacts for Windows (NSIS/MSI), macOS Apple Silicon, and Linux (deb/rpm/AppImage plus a portable tar.gz), generates updater signatures and `latest.json`, and uploads them to a draft Release. Publish it once you have verified the result.

Update packages must be signature-verified. Generate the key once before the first release:

```bash
pnpm tauri signer generate -w ~/.tauri/aurora-launcher.key
```

- Paste the public key into `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`.
- In GitHub `Settings → Secrets and variables → Actions`, add `TAURI_SIGNING_PRIVATE_KEY` (the private key file contents) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

The private key exists only in GitHub Secrets; never commit it, and losing it means you can no longer push updates to existing users. Because `createUpdaterArtifacts` is enabled, a local `pnpm tauri:build` also requires `TAURI_SIGNING_PRIVATE_KEY`; use `cargo check` to verify compilation only.

## IPC commands

| Frontend call | Rust command | Purpose |
| --- | --- | --- |
| `check_all_status` | `check_all_status` | Return dependency, kernel, and Bot status |
| `install_all_deps` | `install_all_deps` | Install Git, uv, Python, pnpm in order |
| `install_dependency` | `install_dependency` | Install a single tool |
| `setToolDir` | `set_tool_dir` | Set / clear a tool's custom install directory |
| `kernel_update` | `kernel_update` | Download or Git-update the kernel |
| `start_bot` | `start_bot` | Prepare the environment and start the Bot |
| `stop_bot` | `stop_bot` | Stop the current Bot |
| `open_app_dir` | `open_app_dir` | Open the runtime directory |
| `open_external_url` | `open_external_url` | Open an external link in the default browser |
| `runtime_info` | `runtime_info` | Return OS, architecture, and runtime directory |
| `checkForUpdates` | `check_launcher_update` | Check for a launcher update |
| `installUpdate` | `install_launcher_update` | Download and install the detected update |

Backend events:

- `progress`: `{ kind, current, total?, label? }`
- `log`: `{ level, message }`
- `bot-state-changed`: `{ running, pid, startedAt, logFile }`

## FAQ

- First install fails: make sure GitHub is reachable; the failure log shows the exact URL.
- Kernel directory modified by hand: the update refuses to continue, to avoid overwriting local work.
- Bot fails to start: check the Bot logs under `tool/logs/`.
- Cannot install Git on macOS / Linux: add the matching portable Git binary to `manifest.rs`; Windows already bundles the official MinGit.
