export interface ToolMeta {
  version: string
  installed: boolean
  installedAt?: string
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
  commit: string
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
  kind: 'git' | 'uv' | 'python' | 'pnpm' | 'clone' | 'sync' | 'update'
  current: number
  total?: number
  label?: string
}

export interface LogLine {
  time: string
  level: 'info' | 'warn' | 'error' | 'success'
  message: string
}

export type ToolKind = 'python' | 'uv' | 'git' | 'pnpm'
