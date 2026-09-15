# AGENTS.md

Tauri 2 desktop launcher for AuroraBot: a Vue 3 (naive-ui) frontend in `src/`, a Rust backend in `src-tauri/src/`. Thin frontend only; every filesystem/process/network op happens in Rust behind Tauri IPC. Architecture and IPC tables are documented in `README.md` (Chinese) — read it before large changes.

## Commands

- `pnpm dev` runs plain Vite in a browser → the app enters **demo mode** (`src/stores/app.ts:100` detects missing `__TAURI_INTERNALS__`): all installs/starts are simulated with fake data, no IPC. To exercise real backend logic use `pnpm tauri:dev`.
- `pnpm tauri:dev` auto-runs `pnpm dev` first (devUrl `http://localhost:1420`, strictPort). Requires Windows WebView2.
- `pnpm build` = `vue-tsc --noEmit && vite build`. There is **no lint script and no JS test runner**; the only frontend verification is this build. Backend: `cargo check`/`cargo clippy` in `src-tauri/`, plus `cargo test` for the few `#[cfg(test)]` unit tests (in `config.rs` and `tools.rs`).
- Release profile uses `lto = "fat"`, `panic = "abort"` → `pnpm tauri:build` is slow.
- Use `src-tauri/target/`; `src-tauri/target-clean/` is a stale untracked copy, ignore it.

## Architecture rules

- Vue must not touch fs/process/network — go through `invoke()`. Command + event names are the contract between `src/stores/app.ts` and Rust; args are camelCase in JS matching Rust arg names (`{ kind }`, `{ headless }`).
- New commands must be registered in `src-tauri/src/main.rs` (`generate_handler!`); new plugin APIs need permission entries in `src-tauri/capabilities/default.json` (currently only `core:default`).
- Install flow is `cache/downloads/` → `cache/staging/` → atomic rename into `tools/<tool>/`. Never write directly into the live tool dir.
- Every external subprocess goes through SandboxRunner (`env_clear()` etc.); binaries are resolved managed-first with a system-PATH fallback (`tools.rs::resolve_exe`). The launcher never installs into or modifies host PATH/registry.
- Tool versions are pinned in `src-tauri/src/manifest.rs` (Python 3.12.7, uv 0.12.10, Git 2.55.0.5, pnpm 9.12.0) **and duplicated as fake demo values in `src/stores/app.ts`** (`demoInstallSteps`) — update both when bumping. Per-platform SHA256s live in `manifest.rs::sha256_of` and are verified on download (including cached archives); uv and MinGit have them, pnpm 9.12.0 does not (its release predates GitHub asset digests).
- The portable Git tool is Windows-only; `manifest.rs` returns an error on macOS/Linux. Install order is git → uv → python → pnpm (uv must precede python, uv installs python).

## Runtime state

- State lives in a portable `tool/` folder next to the executable — `<exe dir>\tool` on Windows — with the sub-tree built in `state.rs::RuntimePaths`. Top level is grouped by purpose: `runtime/` (tools, venv, kernel), `data/` (home, state), `cache/` (uv/pip/npm caches plus downloads/ and staging/), and `logs/`. Old flat layouts are auto-migrated on startup. By default `default_user` uses `~/.aurora-launcher/tool` in the home dir (`directories::BaseDirs`, same path on all platforms), so a read-only install dir (e.g. `/usr/bin` on Linux) is fine; an exe-adjacent `tool/` is used only when that folder already exists (portable copy). Deleting that `tool` dir fully resets the launcher; it never touches host PATH/registry.
- Tool "ready" status is two-source: launcher-managed dir first, else a usable system install found on the host PATH is reused and reported ready (see `tools.rs::system_exe_path`, `probe_version`). `resolve_exe`/`uv_command_exe`/`system_python` route real commands to managed-or-system binaries.
- Frontend edits to a running feature are verified with `pnpm tauri:dev`; backend-only edits with `cargo check` from `src-tauri`.

## Conventions

- UI strings, user-facing log messages, and Rust code comments are written in Chinese; keep new ones consistent.
- `tsconfig.json` is strict with `noUnusedLocals`/`noUnusedParameters`; `vue-tsc` runs on every build so unused code fails `pnpm build`.
- The UI scales with the window: `main.rs::sync_zoom` sets the WebView zoom factor from the logical width relative to `DESIGN_WIDTH` (960, matching `tauri.conf.json`), clamped to `[1.0, MAX_ZOOM]`. It's driven by `RunEvent::WindowEvent::Resized`; don't replace it with CSS `zoom` (that breaks `100vh`).
