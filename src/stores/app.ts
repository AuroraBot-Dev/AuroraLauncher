import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type Event } from '@tauri-apps/api/event'
import type {
  DependencyStatus,
  KernelStatus,
  AuroraProcessInfo,
  ProgressEvent,
  LogLine,
  ToolKind,
  LauncherUpdate,
  RuntimeInfo,
  ThemeMode
} from '../types'

export type BackendMode = 'checking' | 'tauri' | 'preview'

export interface DemoStep {
  kind: string
  label: string
  set: () => void
}

const defaultDependencies = (): DependencyStatus => ({
  python: { version: '', installed: false, source: 'missing', path: '' },
  uv: { version: '', installed: false, source: 'missing', path: '' },
  git: { version: '', installed: false, source: 'missing', path: '' },
  pnpm: { version: '', installed: false, source: 'missing', path: '' }
})

const emptyKernel = (): KernelStatus => ({
  exists: false,
  remote: '',
  branch: '',
  commitShort: '',
  message: '',
  date: ''
})

const sleep = (ms: number) => new Promise<void>((resolve) => window.setTimeout(resolve, ms))

// 启动后延迟自动检查更新的毫秒数；期间若状态刷新占用则顺延重试
const AUTO_CHECK_DELAY_MS = 1500

export const useAppStore = defineStore('app', () => {
  const mode = ref<BackendMode>('checking')

  // 主题：浅色 / 深色 / 跟随系统。跟随系统时监听系统配色变化。
  const storedTheme = localStorage.getItem('aurora-theme')
  const themeMode = ref<ThemeMode>(
    storedTheme === 'light' || storedTheme === 'dark' || storedTheme === 'system' ? storedTheme : 'system'
  )
  const systemDark = ref(
    typeof window !== 'undefined' && typeof window.matchMedia === 'function'
      ? window.matchMedia('(prefers-color-scheme: dark)').matches
      : false
  )
  const isDark = computed(() =>
    themeMode.value === 'system' ? systemDark.value : themeMode.value === 'dark'
  )
  if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (event) => {
      systemDark.value = event.matches
    })
  }

  const dependencyStatus = ref<DependencyStatus>(defaultDependencies())
  const kernelStatus = ref<KernelStatus>(emptyKernel())
  const process = ref<AuroraProcessInfo | null>(null)
  const isBusy = ref(false)
  const progress = ref<ProgressEvent | null>(null)
  const logs = ref<LogLine[]>([])
  const bootTime = ref('')
  const coreDownloading = ref(false)
  const autoCheckUpdate = ref(localStorage.getItem('aurora-auto-update') !== '0')
  const availableUpdate = ref<LauncherUpdate | null>(null)
  const updateDialogVisible = ref(false)
  const checkingUpdate = ref(false)
  const runtimeInfo = ref<RuntimeInfo | null>(null)

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
          source: 'managed',
          path: `runtime/tools/${kind}`,
          installedAt: new Date().toLocaleString('zh-CN', { hour12: false })
        }
      }
    }))
  }

  async function loadRuntimeInfo() {
    if (!withReadyMode()) return
    try {
      runtimeInfo.value = await invoke<RuntimeInfo>('runtime_info')
    } catch (e: any) {
      addLog('error', `获取运行环境信息失败: ${e?.message || e}`)
    }
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
      await loadRuntimeInfo()
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
    if (isBusy.value) return
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

  async function setToolDir(kind: ToolKind, path: string): Promise<void> {
    if (!withReadyMode()) {
      addLog('info', `演示模式：${kind} 安装目录已设为 ${path || '默认'}`)
      return
    }
    try {
      await invoke('set_tool_dir', { kind, path })
      addLog('success', `${kind} 安装目录已更新，重新安装后生效`)
      await refreshAll()
    } catch (e: any) {
      const message = `设置 ${kind} 目录失败: ${e?.message || e}`
      addLog('error', message)
      throw new Error(message)
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

  async function downloadCore() {
    if (coreDownloading.value) return
    coreDownloading.value = true
    try {
      await cloneOrUpdate()
    } finally {
      coreDownloading.value = false
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

  async function openExternal(url: string) {
    if (!withReadyMode()) {
      addLog('warn', `演示模式：无法打开外部链接 ${url}`)
      return
    }
    try {
      await invoke('open_external_url', { url })
    } catch (e: any) {
      addLog('error', `打开链接失败: ${e?.message || e}`)
    }
  }

  async function checkForUpdates(): Promise<LauncherUpdate | null> {
    if (!withReadyMode()) {
      addLog('warn', '演示模式：跳过检查更新（正式版本会查询 GitHub Releases）')
      return null
    }
    try {
      addLog('info', '正在检查 AuroraLauncher 更新…')
      const update = await invoke<LauncherUpdate | null>('check_launcher_update')
      if (update) addLog('info', `发现新版本 ${update.version}（当前 ${update.currentVersion}）`)
      else addLog('info', '当前已是最新版本')
      return update
    } catch (e: any) {
      const raw = e?.message || String(e)
      // GitHub 上还没有任何 Release（或还没有 latest.json）时视为“暂无可更新”；
      // 发布产物缺少当前平台（如 Windows 未构建）也会报 fallback platforms，同样视为无更新
      if (/404|not\s?found|release|no update|fallback platforms/i.test(raw)) {
        addLog('info', '更新源暂无可用发布，当前版本无需更新')
        return null
      }
      const message = `检查更新失败: ${raw}`
      addLog('error', message)
      throw new Error(message)
    }
  }

  async function installUpdate(): Promise<void> {
    if (!withReadyMode()) {
      addLog('warn', '演示模式：无法安装更新')
      return
    }
    isBusy.value = true
    try {
      addLog('info', '开始下载并安装 AuroraLauncher 更新…')
      await invoke('install_launcher_update')
      addLog('success', '更新已安装，应用即将重启')
    } catch (e: any) {
      const message = `安装更新失败: ${e?.message || e}`
      addLog('error', message)
      throw new Error(message)
    } finally {
      isBusy.value = false
    }
  }

  function setThemeMode(value: ThemeMode) {
    themeMode.value = value
    localStorage.setItem('aurora-theme', value)
  }

  function setAutoCheckUpdate(enabled: boolean) {
    autoCheckUpdate.value = enabled
    localStorage.setItem('aurora-auto-update', enabled ? '1' : '0')
  }

  /// 手动检查更新：发现新版本时打开全局更新弹窗，否则返回 null。
  async function requestUpdateCheck(): Promise<LauncherUpdate | null> {
    if (checkingUpdate.value || isBusy.value) return null
    checkingUpdate.value = true
    try {
      const update = await checkForUpdates()
      if (update) {
        availableUpdate.value = update
        updateDialogVisible.value = true
      }
      return update
    } finally {
      checkingUpdate.value = false
    }
  }

  async function runAutoCheck() {
    if (!autoCheckUpdate.value) return
    if (isBusy.value || checkingUpdate.value) {
      window.setTimeout(runAutoCheck, AUTO_CHECK_DELAY_MS)
      return
    }
    try {
      const update = await checkForUpdates()
      if (update) {
        availableUpdate.value = update
        updateDialogVisible.value = true
      }
    } catch {
      // 自动检查失败保持静默，详情已写入日志
    }
  }

  let autoCheckScheduled = false
  function scheduleAutoCheck() {
    if (autoCheckScheduled || mode.value !== 'tauri') return
    autoCheckScheduled = true
    window.setTimeout(runAutoCheck, AUTO_CHECK_DELAY_MS)
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
    isDark,
    themeMode,
    bootTime,
    autoCheckUpdate,
    availableUpdate,
    updateDialogVisible,
    checkingUpdate,
    runtimeInfo,
    refreshAll,
    installAll,
    installOne,
    setToolDir,
    cloneOrUpdate,
    downloadCore,
    startBot,
    stopBot,
    openFolder,
    openExternal,
    checkForUpdates,
    requestUpdateCheck,
    scheduleAutoCheck,
    setAutoCheckUpdate,
    setThemeMode,
    installUpdate,
    addLog,
    demoReset,
    initListeners
  }
})
