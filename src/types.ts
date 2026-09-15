export type ToolSource = 'managed' | 'system' | 'missing'

export interface ToolMeta {
  version: string
  installed: boolean
  source: ToolSource
  path: string
  installedAt?: string
  managedAvailable?: boolean
  systemAvailable?: boolean
}

export interface DependencyStatus {
  python: ToolMeta
  uv: ToolMeta
  git: ToolMeta
  pnpm: ToolMeta
}

export interface KernelStatus {
  exists: boolean
  remote: string
  branch: string
  commitShort: string
  message: string
  date: string
}

export interface AuroraProcessInfo {
  running: boolean
  pid: number
  startedAt: string
  logFile?: string
}

export interface ProgressEvent {
  kind: 'git' | 'uv' | 'python' | 'pnpm' | 'clone' | 'sync' | 'update' | 'launcher'
  current: number
  total?: number
  label?: string
  /// 瞬时下载速度（字节/秒）；只有下载阶段有值
  speed?: number
}

export interface LogLine {
  time: string
  level: 'info' | 'warn' | 'error' | 'success'
  message: string
}

/// 「对话」页的一行：role 决定气泡样式，text 是内容本身
export type ChatRole = 'user' | 'bot' | 'system'

export interface ChatLine {
  role: ChatRole
  text: string
}

export type ToolKind = 'python' | 'uv' | 'git' | 'pnpm'

export interface LauncherUpdate {
  currentVersion: string
  version: string
  date: string | null
  notes: string | null
}

export interface RuntimeInfo {
  os: string
  arch: string
  root: string
  downloadSource: string
  githubMirror: string
}

export type ThemeMode = 'light' | 'dark' | 'system'

export interface EnvEntry {
  name: string
  value: string
  secret: boolean
}

export interface AppEntry {
  package: string
  enabled: boolean
}

export interface LauncherConfig {
  /// 内核是否已初始化（`config/` 已由 setup 生成）；只下载核心时为 false
  initialized: boolean
  env: EnvEntry[]
  apps: AppEntry[]
}
