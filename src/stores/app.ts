import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type Event } from '@tauri-apps/api/event'
import type {
  DependencyStatus,
  KernelStatus,
  AuroraProcessInfo,
  ProgressEvent,
  LogLine,
  ToolKind
} from '../types'

export type BackendMode = 'checking' | 'tauri' | 'preview'

export interface DemoStep {
  kind: string
  label: string
  set: () => void
}

const defaultDependencies = (): DependencyStatus => ({
  python: { version: '', installed: false },
  uv: { version: '', installed: false },
  git: { version: '', installed: false },
  pnpm: { version: '', installed: false }
})

const emptyKernel = (): KernelStatus => ({
  exists: false,
  remote: '',
  branch: '',
  commit: '',
  commitShort: '',
  message: '',
  date: ''
})

const sleep = (ms: number) => new Promise<void>((resolve) => window.setTimeout(resolve, ms))

export const useAppStore = defineStore('app', () => {
  const mode = ref<BackendMode>('checking')
  const dependencyStatus = ref<DependencyStatus>(defaultDependencies())
  const kernelStatus = ref<KernelStatus>(emptyKernel())
  const process = ref<AuroraProcessInfo | null>(null)
  const isBusy = ref(false)
  const progress = ref<ProgressEvent | null>(null)
  const logs = ref<LogLine[]>([])
  const bootTime = ref('')
  const coreDownloading = ref(false)

  function addLog(level: LogLine['level'], message: string) {
    logs.value.push({
      time: new Date().toLocaleTimeString('zh-CN', { hour12: false }),
      level,
      message
    })
    if (logs.value.length > 500) {
      logs.value = logs.value.slice(-500)
    }
  }

  function isTauriRuntime(): boolean {
    return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
  }

  async function detectMode() {
    if (isTauriRuntime()) {
      mode.value = 'tauri'
    } else {
      mode.value = 'preview'
      if (!logs.value.some((line) => line.message.includes('演示模式'))) {
        addLog('warn', '演示模式：当前为浏览器预览，界面数据由前端模拟，不会调用真实运行环境。')
      }
    }
  }

  function withReadyMode(): boolean {
    if (mode.value === 'checking') detectMode()
    return mode.value === 'tauri'
  }

  async function runDemoStep(step: DemoStep) {
    isBusy.value = true
    addLog('info', `开始 ${step.label} ...`)
    for (let current = 0; current <= 100; current += 7 + Math.floor(Math.random() * 8)) {
      progress.value = {
        kind: step.kind as ProgressEvent['kind'],
        current: Math.min(100, current),
        total: 100,
        label: step.label
      }
      await sleep(110)
    }
    step.set()
    addLog('success', `${step.label} 完成`)
    progress.value = null
    isBusy.value = false
  }

  async function runDemoSteps(steps: DemoStep[]) {
    for (const step of steps) {
      await runDemoStep(step)
    }
  }

  function demoInstallSteps(kinds: ToolKind[]): DemoStep[] {
    const labels: Record<ToolKind, string> = {
      git: 'Git 运行时',
      python: 'Python 解释器',
      uv: 'uv 包管理器',
      pnpm: 'pnpm 包管理器'
    }
    return kinds.map((kind) => ({
      kind,
      label: `安装 ${labels[kind]}`,
      set: () => {
        dependencyStatus.value[kind] = {
          version: {
            git: '2.46.0',
            python: '3.12.7',
            uv: '0.12.10',
            pnpm: '9.12.0'
          }[kind],
          installed: true,
          installedAt: new Date().toLocaleString('zh-CN', { hour12: false })
        }
      }
    }))
  }

  async function refreshAll() {
    if (!withReadyMode()) {
      if (logs.value.length === 0) {
        addLog('info', '正在检查运行环境 ...')
        await sleep(500)
      }
      return
    }
    try {
      isBusy.value = true
      const result = await invoke<{
        dependency: DependencyStatus
        kernel: KernelStatus
        process: AuroraProcessInfo | null
      }>('check_all_status')
      dependencyStatus.value = result.dependency
      kernelStatus.value = result.kernel
      process.value = result.process
    } catch (e: any) {
      addLog('error', `刷新状态失败: ${e?.message || e}`)
    } finally {
      isBusy.value = false
    }
  }

  async function installAll() {
    if (!withReadyMode()) {
      await runDemoSteps(
        demoInstallSteps(['git', 'uv', 'python', 'pnpm']).filter(
          (step) => !dependencyStatus.value[step.kind as ToolKind].installed
        )
      )
      await refreshAll()
      return
    }
    try {
      isBusy.value = true
      addLog('info', '开始安装全部依赖...')
      await invoke('install_all_deps')
      addLog('success', '依赖安装完成')
      await refreshAll()
    } catch (e: any) {
      const message = `安装依赖失败: ${e?.message || e}`
      addLog('error', message)
      throw new Error(message)
    } finally {
      isBusy.value = false
    }
  }

  async function installOne(kind: ToolKind, force = false) {
    if (!withReadyMode()) {
      const order: ToolKind[] =
        kind === 'python' ? ['uv', 'python'] : kind === 'uv' ? ['uv'] : [kind]
      const filtered = demoInstallSteps(order).filter(
        (step) => !dependencyStatus.value[step.kind as ToolKind].installed
      )
      await runDemoSteps(filtered)
      await refreshAll()
      return
    }
    try {
      isBusy.value = true
      addLog('info', `开始${force ? '重' : ''}安装 ${kind}...`)
      await invoke('install_dependency', { kind, force })
      addLog('success', `${kind} 安装完成`)
      await refreshAll()
    } catch (e: any) {
      const message = `安装 ${kind} 失败: ${e?.message || e}`
      addLog('error', message)
      throw new Error(message)
    } finally {
      isBusy.value = false
    }
  }

  async function cloneOrUpdate() {
    if (!withReadyMode()) {
      const existing = kernelStatus.value.exists
      await runDemoSteps([
        {
          kind: 'clone',
          label: existing ? '拉取内核最新代码' : '克隆 AuroraBot 内核仓库',
          set: () => {
            kernelStatus.value = {
              exists: true,
              remote: 'https://github.com/AuroraBot/AuroraBot.git',
              branch: 'main',
              commit: '9f6d4a2b7c81e0f3d5a76c2b8f4a901c',
              commitShort: '9f6d4a2',
              message: 'chore: 保持内核与运行时契约同步',
              date: new Date().toLocaleString('zh-CN', { hour12: false })
            }
          }
        },
        {
          kind: 'sync',
          label: '使用 uv 同步内核 Python 依赖',
          set: () => addLog('info', '虚拟环境已就绪：runtime/env/aurora')
        }
      ])
      await refreshAll()
      return
    }
    try {
      isBusy.value = true
      addLog('info', kernelStatus.value.exists ? '开始更新内核源码...' : '开始下载 AuroraBot 内核...')
      await invoke('kernel_update')
      addLog('success', '内核更新完成')
      await refreshAll()
    } catch (e: any) {
      const message = `内核操作失败: ${e?.message || e}`
      addLog('error', message)
      throw new Error(message)
    } finally {
      isBusy.value = false
    }
  }

  async function startBot(headless = true) {
    if (!withReadyMode()) {
      await runDemoStep({
        kind: 'sync',
        label: '启动 AuroraBot 内核进程',
        set: () => {
          process.value = {
            running: true,
            pid: 48216,
            startedAt: new Date().toLocaleString('zh-CN', { hour12: false }),
            logFile: 'runtime/logs/aurora-bot.log'
          }
        }
      })
      await refreshAll()
      return
    }
    try {
      isBusy.value = true
      addLog('info', `启动 AuroraBot (headless=${headless})...`)
      await invoke('start_bot', { headless })
      addLog('success', 'AuroraBot 已启动')
      await refreshAll()
    } catch (e: any) {
      const message = `启动 Bot 失败: ${e?.message || e}`
      addLog('error', message)
      throw new Error(message)
    } finally {
      isBusy.value = false
    }
  }

  async function stopBot() {
    if (!withReadyMode()) {
      isBusy.value = true
      addLog('info', '正在停止 AuroraBot 进程 ...')
      await sleep(450)
      process.value = null
      addLog('success', 'AuroraBot 已停止')
      isBusy.value = false
      await refreshAll()
      return
    }
    try {
      isBusy.value = true
      addLog('info', '停止 AuroraBot...')
      await invoke('stop_bot')
      addLog('success', 'AuroraBot 已停止')
      await refreshAll()
    } catch (e: any) {
      const message = `停止 Bot 失败: ${e?.message || e}`
      addLog('error', message)
      throw new Error(message)
    } finally {
      isBusy.value = false
    }
  }

  async function openFolder() {
    if (!withReadyMode()) {
      addLog('warn', '演示模式无法打开真实目录，正式版本会在系统文件管理器中打开 runtime/')
      return
    }
    try {
      await invoke('open_app_dir')
    } catch (e: any) {
      addLog('error', `打开目录失败: ${e?.message || e}`)
    }
  }

  function demoReset() {
    if (mode.value !== 'preview') return
    dependencyStatus.value = defaultDependencies()
    kernelStatus.value = emptyKernel()
    process.value = null
    progress.value = null
    logs.value = []
    addLog('info', '演示状态已重置，可重新体验初始化流程。')
  }

  function setCoreDownloading(downloading: boolean) {
    coreDownloading.value = downloading
  }

  async function initListeners() {
    await detectMode()
    if (!isTauriRuntime()) return
    await listen<ProgressEvent>('progress', (event: Event<ProgressEvent>) => {
      const payload = event.payload
      // 后端用 {current:0, total:null, label:null} 表示“清除进度”；
      // 收到即视为无进行中的安装/下载，避免底部残留一条空转进度条
      if (payload.total == null && payload.label == null) {
        progress.value = null
      } else {
        progress.value = payload
      }
    })
    await listen<{ level: LogLine['level']; message: string }>(
      'log',
      (event: Event<{ level: LogLine['level']; message: string }>) => {
        addLog(event.payload.level, event.payload.message)
      }
    )
    await listen<AuroraProcessInfo>(
      'bot-state-changed',
      (event: Event<AuroraProcessInfo>) => {
        process.value = event.payload
      }
    )
  }

  return {
    dependencyStatus,
    kernelStatus,
    process,
    isBusy,
    progress,
    logs,
    coreDownloading,
    mode,
    bootTime,
    refreshAll,
    installAll,
    installOne,
    cloneOrUpdate,
    startBot,
    stopBot,
    openFolder,
    addLog,
    demoReset,
    setCoreDownloading,
    initListeners
  }
})
