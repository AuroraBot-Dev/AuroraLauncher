# AuroraLauncher

[中文](README.md) | [English](README.en.md) | 日本語

AuroraBot の Tauri デスクトップランチャーです。ランタイムツール（Python / uv / Git / pnpm）を自身のディレクトリにダウンロードし、AuroraBot カーネルを Git リポジトリとして更新し、Bot を制御された方法で起動します。ホストの PATH・レジストリ・グローバルな Python 環境には一切変更を加えません。

UI は薄い Vue 3（Naive UI）フロントエンドで、ファイルシステム / プロセス / ネットワーク操作はすべて Rust バックエンド側で Tauri IPC を介して行われます。

## 前提

- Node.js 20+
- pnpm 10+
- Rust stable
- Windows は WebView2 ランタイムが必要

## ローカル開発

依存関係のインストール:

```bash
pnpm install
```

ブラウザで UI をプレビュー。このモードは**デモモード**で、UI のデータはフロントエンドでシミュレートされ、インストール / 起動はバックエンドを呼ばず、マシンにも変更を加えません。

```bash
pnpm dev
```

実際の Tauri 開発ウィンドウを起動（先に `pnpm dev` を実行。devUrl は `http://localhost:1420` 固定）:

```bash
pnpm tauri:dev
```

フロントエンドの型チェックとビルドのみ:

```bash
pnpm build          # vue-tsc --noEmit && vite build
```

Rust バックエンドのみチェック（フルパッケージより速く、署名キーも不要）:

```bash
cd src-tauri
cargo check
```

## ランタイムディレクトリ

既定ではランタイムデータはホームディレクトリの `~/.aurora-launcher/tool`（全プラットフォーム共通）に置かれます。読み取り専用のインストール先（Linux の `/usr/bin` など）でも問題ありません。実行ファイルと同じ階層に `tool/` が**すでに存在する**場合（ポータブル版）はそちらを使用し、exe と `tool/` をまとめてコピーすれば移行できます。`tool/` を丸ごと削除すればリセットされ、書き込みのために root 権限を要求しません。

```text
<exe のあるディレクトリ>/
├── aurora-launcher.exe
└── tool/                  ランタイムルート（用途別に 4 フォルダ）
    ├── runtime/           実行に必須
    │   ├── tools/         管理下のツール本体：git / uv / python / pnpm
    │   ├── venv/          AuroraBot 専用 venv（ベース解釈器 + 依存）
    │   └── kernel/auroraBot/  AuroraBot Git リポジトリ（docs / panel サブモジュール含む）
    ├── data/              ユーザーデータ
    │   ├── home/          サンドボックス HOME：AppData、temp、.gitconfig
    │   └── state/         settings.json とインストール記録
    ├── cache/             削除可能なキャッシュ（自動再生成）
    │   ├── uv/ pip/ npm/   各ツールのパッケージキャッシュ
    │   ├── downloads/      ダウンロードしたアーカイブ
    │   └── staging/        未完了インストールのステージング
    └── logs/              Bot とランチャーのログ
```

補足：`runtime/tools/` はツールの**本体**、`cache/uv` は uv の**パッケージキャッシュ**で別物です。`runtime/venv` は仮想環境で、その `Scripts/python.exe` はベース解釈器への入口にすぎません（2 つ目の Python ではありません）。ベースがシステム Python の場合、`runtime/tools/python/` は空のままです。

## アーキテクチャ

Vue は表示と操作のみを担当し、Rust の `AppState` が唯一の状態源です：

```text
Vue UI ──invoke / event──► Tauri コマンド層 ──► AppState
                                                 ├── ToolService   Python/uv/Git/pnpm のダウンロードと管理
                                                 ├── KernelService AuroraBot カーネルの clone / 更新
                                                 ├── BotService    venv 作成、uv sync、Bot の起動 / 停止
                                                 └── SandboxRunner 子プロセスごとにクリーンな環境を再構築
```

主な制約：

- インストールは常に `cache/downloads/ → cache/staging/ → tools/<tool>/ へアトミックにリネーム` の順で、正式ディレクトリへ直接書き込みません。
- すべての子プロセスは SandboxRunner を通ります（`env_clear()`、PATH / HOME / キャッシュはすべて `tool/` を指す）。
- ツールの準備状態は管理ディレクトリを優先し、無ければホスト PATH 上の利用可能な版を再利用、どちらも無い場合のみダウンロードします。
- カーネル更新前にローカルの tracked 変更を確認し、ユーザーの変更を上書きしません。

ツールのバージョンは `src-tauri/src/manifest.rs` で固定：Python `3.12.7`、uv `0.12.10`、Git `2.55.0.5`、pnpm `9.12.0`。

## 自動更新とリリース

アプリは公式の Tauri updater（`tauri-plugin-updater`）を使用します。起動時に一度サイレントで確認し、新バージョンがあればダイアログで通知します。「About → 更新を確認」でいつでも手動確認できます。確認先は `src-tauri/tauri.conf.json` に設定された、本リポジトリの Release を指す `latest.json` です。

バージョンの単一の情報源はルート `package.json` の `version` です（`tauri.conf.json` は `"version": "../package.json"` で参照）：

```bash
pnpm version 0.2.0
git commit -am "chore: bump version to 0.2.0"
git push origin main
git tag v0.2.0 && git push origin v0.2.0
```

`v*` タグを push すると、`.github/workflows/release.yml` が Windows（NSIS/MSI）、macOS Apple Silicon、Linux（deb/rpm/AppImage とポータブル tar.gz）向けにビルドし、updater 署名と `latest.json` を生成して draft Release にアップロードします。確認後に「公開」してください。

更新パッケージは署名検証が必須です。初回リリース前に一度だけ鍵を生成します：

```bash
pnpm tauri signer generate -w ~/.tauri/aurora-launcher.key
```

- 公開鍵を `src-tauri/tauri.conf.json` の `plugins.updater.pubkey` に貼り付けます。
- GitHub の `Settings → Secrets and variables → Actions` に `TAURI_SIGNING_PRIVATE_KEY`（秘密鍵ファイルの内容）と `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` を追加します。

秘密鍵は GitHub Secrets にのみ存在します。コミットしないでください。紛失すると既存ユーザーへ更新を配信できなくなります。`createUpdaterArtifacts` を有効にしているため、ローカルの `pnpm tauri:build` でも `TAURI_SIGNING_PRIVATE_KEY` が必要です。コンパイル確認だけなら `cargo check` を使ってください。

## IPC コマンド

| フロント呼び出し | Rust コマンド | 役割 |
| --- | --- | --- |
| `check_all_status` | `check_all_status` | 依存関係・カーネル・Bot の状態を返す |
| `install_dependency` | `install_dependency` | 単一ツールをインストール |
| `setToolDir` | `set_tool_dir` | ツールのカスタムインストール先を設定 / 解除 |
| `kernel_update` | `kernel_update` | カーネルをダウンロードまたは Git 更新 |
| `start_bot` | `start_bot` | 環境を準備して Bot を起動 |
| `stop_bot` | `stop_bot` | 現在の Bot を停止 |
| `open_app_dir` | `open_app_dir` | ランタイムディレクトリを開く |
| `open_external_url` | `open_external_url` | 外部リンクを既定のブラウザで開く |
| `runtime_info` | `runtime_info` | OS・アーキテクチャ・ランタイムディレクトリを返す |
| `checkForUpdates` | `check_launcher_update` | ランチャー自身の更新を確認 |
| `installUpdate` | `install_launcher_update` | 検出した更新をダウンロードしてインストール |

バックエンドイベント：

- `progress`：`{ kind, current, total?, label? }`
- `log`：`{ level, message }`
- `bot-state-changed`：`{ running, pid, startedAt, logFile }`

## よくある質問

- 初回インストールが失敗する：GitHub に到達できるか確認してください。失敗ログに具体的な URL が出ます。
- カーネルディレクトリを手動で変更した：ローカルの作業を上書きしないよう、更新は続行を拒否します。
- Bot が起動しない：`tool/logs/` 配下の Bot ログを確認してください。
- macOS / Linux で Git をインストールできない：`manifest.rs` に対応するポータブル Git バイナリを追加してください。Windows は公式 MinGit を同梱済みです。
