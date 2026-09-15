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
  ChatRole,
  ChatLine,
  ToolKind,
  LauncherUpdate,
  RuntimeInfo,
  ThemeMode,
  LauncherConfig
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
  const coreDownloading = ref(false)
  const kernelAbout = ref('')
  const loadingKernelAbout = ref(false)
  const autoCheckUpdate = ref(localStorage.getItem('aurora-auto-update') !== '0')
  const availableUpdate = ref<LauncherUpdate | null>(null)
  const updateDialogVisible = ref(false)
  const checkingUpdate = ref(false)
  const runtimeInfo = ref<RuntimeInfo | null>(null)
  const launcherConfig = ref<LauncherConfig | null>(null)
  const loadingConfig = ref(false)
  const downloadSource = computed(() => runtimeInfo.value?.downloadSource ?? 'mirror')
  const githubMirror = computed(() => runtimeInfo.value?.githubMirror ?? '')
  const chatLines = ref<ChatLine[]>([])

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

  function detectMode() {
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
            git: '2.55.0.5',
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

  interface ActionOptions<T> {
    /// 真实模式下调用的 Tauri 命令
    run: () => Promise<T>
    /// 演示模式下的模拟行为
    demo?: () => void | Promise<void>
    /// 失败日志前缀，同时作为抛出错误的文案前缀
    failPrefix: string
    /// 开始日志（仅真实模式）
    startLog?: string
    /// 成功日志（仅真实模式）
    successLog?: string
    /// 是否占用 isBusy，默认 true
    busy?: boolean
    /// 完成后的刷新动作，默认 refreshAll；传 false 表示不刷新
    refresh?: (() => void | Promise<void>) | false
  }

  /// 统一「真实模式执行命令 / 演示模式走模拟」的样板：isBusy、开始/成功/失败日志、
  /// 完成后刷新。演示分支只提供模拟行为，不再和真实分支成对复制。
  async function runAction<T>(options: ActionOptions<T>): Promise<T | undefined> {
    if (!withReadyMode()) {
      if (options.demo) await options.demo()
      return undefined
    }
    const busy = options.busy ?? true
    if (busy) isBusy.value = true
    if (options.startLog) addLog('info', options.startLog)
    try {
      const result = await options.run()
      if (options.successLog) addLog('success', options.successLog)
      const refresh = options.refresh === false ? null : options.refresh ?? refreshAll
      if (refresh) await refresh()
      return result
    } catch (e: any) {
      const message = `${options.failPrefix}: ${e?.message || e}`
      addLog('error', message)
      throw new Error(message)
    } finally {
      if (busy) isBusy.value = false
    }
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

  async function installOne(kind: ToolKind, force = false) {
    if (isBusy.value) return
    await runAction({
      startLog: `开始${force ? '重' : ''}安装 ${kind}...`,
      successLog: `${kind} 安装完成`,
      failPrefix: `安装 ${kind} 失败`,
      run: () => invoke('install_dependency', { kind, force }),
      demo: async () => {
        const order: ToolKind[] =
          kind === 'python' ? ['uv', 'python'] : kind === 'uv' ? ['uv'] : [kind]
        const filtered = demoInstallSteps(order).filter(
          (step) => !dependencyStatus.value[step.kind as ToolKind].installed
        )
        await runDemoSteps(filtered)
        await refreshAll()
      }
    })
  }

  async function setToolDir(kind: ToolKind, path: string): Promise<void> {
    await runAction({
      busy: false,
      successLog: `${kind} 安装目录已更新，重新安装后生效`,
      failPrefix: `设置 ${kind} 目录失败`,
      run: () => invoke('set_tool_dir', { kind, path }),
      demo: () => addLog('info', `演示模式：${kind} 安装目录已设为 ${path || '默认'}`)
    })
  }

  function addChatLine(role: ChatRole, text: string) {
    chatLines.value.push({ role, text })
    if (chatLines.value.length > 1000) {
      chatLines.value = chatLines.value.slice(-1000)
    }
  }

  function clearChat() {
    chatLines.value = []
  }

  function clearLogs() {
    logs.value = []
  }

  /// 往 Bot 的 stdin 写入一行对话输入。
  async function sendChatInput(text: string): Promise<void> {
    const value = text.trim()
    if (!value) return
    addChatLine('user', value)
    if (!withReadyMode()) {
      addChatLine('bot', '（演示模式：无真实 Bot 回复）')
      return
    }
    try {
      await invoke('send_bot_input', { text: value })
    } catch (e: any) {
      addChatLine('system', `启动器错误: ${e?.message || e}`)
      throw new Error(e?.message || '发送失败')
    }
  }

  /// 手动运行启动器 setup：初始化内核（工具/克隆/子模块/配置/venv/依赖）。
  async function runSetup(): Promise<void> {
    await runAction({
      startLog: '开始初始化内核...',
      successLog: '内核初始化完成',
      failPrefix: '内核初始化失败',
      run: () => invoke('run_setup'),
      demo: () => addLog('info', '演示模式：跳过内核初始化')
    })
  }

  /// 设置包下载源：'mirror'（国内镜像）或 'official'。
  async function setDownloadSource(source: string): Promise<void> {
    await runAction({
      busy: false,
      refresh: loadRuntimeInfo,
      successLog: `下载源已切换为${source === 'official' ? '官方源' : '国内镜像'}`,
      failPrefix: '切换下载源失败',
      run: () => invoke('set_download_source', { source }),
      demo: () => addLog('info', `演示模式：下载源已设为 ${source}`)
    })
  }

  /// 设置 GitHub 加速前缀（留空 = 直连 github.com）。只影响工具安装包与 Python 解释器。
  async function setGithubMirror(mirror: string): Promise<void> {
    await runAction({
      busy: false,
      refresh: loadRuntimeInfo,
      successLog: mirror.trim() ? 'GitHub 加速前缀已保存' : '已恢复直连 github.com',
      failPrefix: '保存加速前缀失败',
      run: () => invoke('set_github_mirror', { mirror }),
      demo: () => addLog('info', `演示模式：加速前缀已设为 ${mirror.trim() || '直连'}`)
    })
  }

  /// 选择工具来源：useSystem=true 优先系统版本，false 优先启动器副本。
  async function setToolSource(kind: ToolKind, useSystem: boolean): Promise<void> {
    await runAction({
      busy: false,
      successLog: `${kind} 来源已切换为${useSystem ? '系统版本' : '启动器副本'}`,
      failPrefix: `切换 ${kind} 来源失败`,
      run: () => invoke('set_tool_source', { kind, useSystem }),
      demo: () =>
        addLog('info', `演示模式：${kind} 来源已设为 ${useSystem ? '系统版本' : '启动器副本'}`)
    })
  }

  async function cloneOrUpdate() {
    await runAction({
      startLog: kernelStatus.value.exists ? '开始更新内核源码...' : '开始下载 AuroraBot 内核...',
      successLog: '内核更新完成',
      failPrefix: '内核操作失败',
      run: () => invoke('kernel_update'),
      demo: async () => {
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
            set: () => addLog('info', '虚拟环境已就绪：runtime/venv')
          }
        ])
        await refreshAll()
      }
    })
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

  async function startBot() {
    await runAction({
      startLog: '启动 AuroraBot...',
      successLog: 'AuroraBot 已启动',
      failPrefix: '启动 Bot 失败',
      run: async () => {
        // 启动前清空对话，避免残留上次会话的内容
        chatLines.value = []
        await invoke('start_bot')
      },
      demo: async () => {
        await runDemoStep({
          kind: 'sync',
          label: '启动 AuroraBot 内核进程',
          set: () => {
            process.value = {
              running: true,
              pid: 48216,
              startedAt: new Date().toLocaleString('zh-CN', { hour12: false }),
              logFile: 'logs/aurora-bot.log'
            }
          }
        })
        await refreshAll()
      }
    })
  }

  async function stopBot() {
    await runAction({
      startLog: '停止 AuroraBot...',
      successLog: 'AuroraBot 已停止',
      failPrefix: '停止 Bot 失败',
      run: () => invoke('stop_bot'),
      demo: async () => {
        isBusy.value = true
        addLog('info', '正在停止 AuroraBot 进程 ...')
        await sleep(450)
        process.value = null
        addLog('success', 'AuroraBot 已停止')
        isBusy.value = false
        await refreshAll()
      }
    })
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

  /// 在文件管理器中定位某工具的实际安装目录。
  async function openToolDir(kind: ToolKind) {
    if (!withReadyMode()) {
      addLog('warn', `演示模式：无法打开 ${kind} 目录`)
      return
    }
    try {
      await invoke('open_tool_dir', { kind })
    } catch (e: any) {
      addLog('error', `打开 ${kind} 目录失败: ${e?.message || e}`)
    }
  }

  /// 在文件管理器中定位 AuroraBot 内核目录。
  async function openKernelDir() {
    if (!withReadyMode()) {
      addLog('warn', '演示模式：无法打开内核目录')
      return
    }
    try {
      await invoke('open_kernel_dir')
    } catch (e: any) {
      addLog('error', `打开内核目录失败: ${e?.message || e}`)
    }
  }

  /// 运行内核的 `aurora about`，把描述文本缓存起来供内核页展示。
  async function loadKernelAbout() {
    if (loadingKernelAbout.value) return
    if (!withReadyMode()) {
      kernelAbout.value =
        '演示模式：此处将展示内核 `aurora about` 的输出，包含内核名称、版本与功能简介。'
      return
    }
    loadingKernelAbout.value = true
    try {
      kernelAbout.value = await invoke<string>('kernel_about')
      addLog('info', '已读取内核描述')
    } catch (e: any) {
      const message = `获取内核描述失败: ${e?.message || e}`
      addLog('error', message)
      throw new Error(message)
    } finally {
      loadingKernelAbout.value = false
    }
  }

  /// 读取内核运行配置：需要在 .env 填写的变量，以及 apps.toml 里的平台。
  async function loadLauncherConfig(): Promise<void> {
    if (!withReadyMode()) {
      launcherConfig.value = {
        initialized: true,
        env: [
          { name: 'DEEPSEEK_API_KEY', value: '', secret: true },
          { name: 'AURORA_QQ_TOKEN', value: '', secret: true }
        ],
        apps: [
          { package: 'org.aurora.clock', enabled: false },
          { package: 'com.github.windows_mcp', enabled: false },
          { package: 'org.aurora.qq', enabled: false }
        ]
      }
      return
    }
    loadingConfig.value = true
    try {
      launcherConfig.value = await invoke<LauncherConfig>('read_launcher_config')
    } catch (e: any) {
      addLog('error', `读取配置失败: ${e?.message || e}`)
      throw new Error(e?.message || '读取配置失败')
    } finally {
      loadingConfig.value = false
    }
  }

  async function setEnvValue(name: string, value: string): Promise<void> {
    await runAction({
      busy: false,
      refresh: false,
      successLog: `${name} 已保存`,
      failPrefix: `保存 ${name} 失败`,
      run: () => invoke('set_env_value', { name, value }),
      demo: () => addLog('info', `演示模式：${name} 已保存`)
    })
  }

  async function setAppEnabled(pkg: string, enabled: boolean): Promise<void> {
    if (!withReadyMode()) {
      addLog('info', `演示模式：${pkg} enabled=${enabled}`)
      return
    }
    try {
      await invoke('set_app_enabled', { package: pkg, enabled })
      addLog(enabled ? 'success' : 'info', `${pkg} 已${enabled ? '启用' : '停用'}`)
    } catch (e: any) {
      const message = `切换 ${pkg} 失败: ${e?.message || e}`
      addLog('error', message)
      throw new Error(message)
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

  let listenersInitialized = false
  async function initListeners() {
    // main.ts 与 App.vue 都会调用本函数，先占位再 await，避免竞态下重复注册
    // 导致每条日志出现两次
    if (listenersInitialized) return
    listenersInitialized = true
    detectMode()
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
    await listen<string>('bot-output', (event: Event<string>) => {
      addChatLine('bot', event.payload)
    })
  }

  return {
    dependencyStatus,
    kernelStatus,
    process,
    isBusy,
    progress,
    logs,
    coreDownloading,
    kernelAbout,
    loadingKernelAbout,
    mode,
    isDark,
    themeMode,
    autoCheckUpdate,
    availableUpdate,
    updateDialogVisible,
    checkingUpdate,
    runtimeInfo,
    launcherConfig,
    loadingConfig,
    loadLauncherConfig,
    setEnvValue,
    setAppEnabled,
    downloadSource,
    setDownloadSource,
    githubMirror,
    setGithubMirror,
    runSetup,
    chatLines,
    sendChatInput,
    clearChat,
    clearLogs,
    refreshAll,
    installOne,
    setToolDir,
    setToolSource,
    cloneOrUpdate,
    downloadCore,
    startBot,
    stopBot,
    openFolder,
    openToolDir,
    openKernelDir,
    loadKernelAbout,
    openExternal,
    checkForUpdates,
    requestUpdateCheck,
    scheduleAutoCheck,
    setAutoCheckUpdate,
    setThemeMode,
    installUpdate,
    addLog,
    initListeners
  }
})
