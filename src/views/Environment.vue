<template>
  <n-flex vertical :size="16">
    <n-flex align="center" justify="space-between" :wrap="false">
      <n-flex vertical :size="2">
        <n-text strong style="font-size: 18px">运行环境</n-text>
        <n-text depth="3" style="font-size: 12px">
          启动器会优先使用电脑上已有的工具，只有缺失时才需要下载安装。
        </n-text>
      </n-flex>
      <n-space :size="8" :wrap="false">
        <n-tag :type="readyCount === 4 ? 'info' : 'warning'" size="small" :bordered="false">
          {{ readyCount }}/4 就绪
        </n-tag>
        <n-button size="small" :loading="refreshing" @click="refresh">刷新</n-button>
      </n-space>
    </n-flex>

    <TaskProgress boxed :kinds="toolKinds" />

    <div class="tool-grid">
      <n-card v-for="card in dependencyCards" :key="card.key" size="small">
        <template #header>
          <n-flex align="center" :size="8">
            <n-text strong>{{ card.name }}</n-text>
            <n-tag size="small" :bordered="false" :type="sourceTagType(card.source)">
              {{ sourceLabel(card.source) }}
            </n-tag>
          </n-flex>
        </template>
        <template #header-extra>
          <n-text v-if="card.installedAt" depth="3" style="font-size: 12px">
            {{ card.installedAt }}
          </n-text>
        </template>

        <n-flex vertical :size="10">
          <n-text depth="3" style="font-size: 12px">
            {{ formatToolVersion(card.version, card.installed) }}
          </n-text>
          <n-text class="path-line" depth="3">
            {{ card.path || '—' }}
          </n-text>

          <n-flex justify="end" :size="8" :wrap="false">
            <n-button v-if="card.installed" size="small" @click="appStore.openToolDir(card.key)">
              打开位置
            </n-button>
            <n-button v-if="!card.installed" size="small" type="primary" :loading="busyKey === card.key"
              :disabled="botRunning || appStore.isBusy" @click="installMissing(card)">
              安装
            </n-button>
            <n-dropdown v-if="card.installed" trigger="click" :options="menuOptions(card)"
              :disabled="botRunning || appStore.isBusy" @select="(key) => onMenu(card, key)">
              <n-button size="small" :loading="busyKey === card.key"
                :disabled="botRunning || appStore.isBusy">更多</n-button>
            </n-dropdown>
          </n-flex>
        </n-flex>
      </n-card>
    </div>
  </n-flex>

  <n-modal v-model:show="installVisible" preset="card"
    :title="installTarget
      ? (installMode === 'reinstall' ? `重新安装 ${installTarget.name}` : `安装独立副本 ${installTarget.name}`)
      : '安装'" style="width: 460px">
    <n-space vertical :size="8">
      <n-text v-if="installMode === 'reinstall'" depth="3">
        将删除启动器安装的 {{ installTarget?.name }} 并重新下载。可指定安装位置，留空则使用默认位置。
      </n-text>
      <n-text v-else depth="3">
        将下载一份由启动器管理的 {{ installTarget?.name }}，与电脑上已有的互不影响。
        可指定安装位置，留空则使用默认位置。
      </n-text>
      <n-input v-model:value="installPath"
        :placeholder="installTarget ? defaultDependencyPath(installTarget.key) : ''" clearable />
    </n-space>
    <template #footer>
      <n-space justify="end" :size="8">
        <n-button size="small" quaternary @click="installVisible = false">取消</n-button>
        <n-button size="small" type="primary" @click="confirmInstall">
          {{ installMode === 'reinstall' ? '重新安装' : '安装' }}
        </n-button>
      </n-space>
    </template>
  </n-modal>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import {
  NButton,
  NCard,
  NDropdown,
  NFlex,
  NInput,
  NModal,
  NSpace,
  NTag,
  NText,
  useMessage
} from 'naive-ui'
import type { DropdownOption } from 'naive-ui'
import { useAppStore } from '../stores/app'
import { formatToolVersion } from '../format'
import TaskProgress from '../components/TaskProgress.vue'
import type { ToolKind, ToolSource } from '../types'

const appStore = useAppStore()
const message = useMessage()

// 本页只显示工具安装的进度；内核/启动/更新有自己的进度条
const toolKinds = ['git', 'uv', 'python', 'pnpm']

const busyKey = ref<ToolKind | null>(null)
const refreshing = ref(false)
const botRunning = computed(() => appStore.process?.running ?? false)

type InstallMode = 'takeover' | 'reinstall'
const installTarget = ref<DependencyCard | null>(null)
const installPath = ref('')
const installVisible = ref(false)
const installMode = ref<InstallMode>('takeover')

interface DependencyCard {
  key: ToolKind
  name: string
  installed: boolean
  source: ToolSource
  version: string
  path: string
  installedAt?: string
  managedAvailable: boolean
  systemAvailable: boolean
}

const dependencyCards = computed<DependencyCard[]>(() => [
  {
    key: 'git',
    name: 'Git',
    installed: appStore.dependencyStatus.git.installed,
    source: appStore.dependencyStatus.git.source,
    version: appStore.dependencyStatus.git.version,
    path: appStore.dependencyStatus.git.path,
    installedAt: appStore.dependencyStatus.git.installedAt,
    managedAvailable: appStore.dependencyStatus.git.managedAvailable ?? false,
    systemAvailable: appStore.dependencyStatus.git.systemAvailable ?? false
  },
  {
    key: 'uv',
    name: 'uv',
    installed: appStore.dependencyStatus.uv.installed,
    source: appStore.dependencyStatus.uv.source,
    version: appStore.dependencyStatus.uv.version,
    path: appStore.dependencyStatus.uv.path,
    installedAt: appStore.dependencyStatus.uv.installedAt,
    managedAvailable: appStore.dependencyStatus.uv.managedAvailable ?? false,
    systemAvailable: appStore.dependencyStatus.uv.systemAvailable ?? false
  },
  {
    key: 'python',
    name: 'Python',
    installed: appStore.dependencyStatus.python.installed,
    source: appStore.dependencyStatus.python.source,
    version: appStore.dependencyStatus.python.version,
    path: appStore.dependencyStatus.python.path,
    installedAt: appStore.dependencyStatus.python.installedAt,
    managedAvailable: appStore.dependencyStatus.python.managedAvailable ?? false,
    systemAvailable: appStore.dependencyStatus.python.systemAvailable ?? false
  },
  {
    key: 'pnpm',
    name: 'pnpm',
    installed: appStore.dependencyStatus.pnpm.installed,
    source: appStore.dependencyStatus.pnpm.source,
    version: appStore.dependencyStatus.pnpm.version,
    path: appStore.dependencyStatus.pnpm.path,
    installedAt: appStore.dependencyStatus.pnpm.installedAt,
    managedAvailable: appStore.dependencyStatus.pnpm.managedAvailable ?? false,
    systemAvailable: appStore.dependencyStatus.pnpm.systemAvailable ?? false
  }
])

const readyCount = computed(() => dependencyCards.value.filter((card) => card.installed).length)

function sourceLabel(source: ToolSource): string {
  if (source === 'managed') return '启动器已安装'
  if (source === 'system') return '系统提供'
  return '未安装'
}

function sourceTagType(source: ToolSource): 'info' | 'success' | 'warning' {
  if (source === 'managed') return 'success'
  if (source === 'system') return 'info'
  return 'warning'
}

function defaultDependencyPath(kind: ToolKind) {
  return `runtime/tools/${kind}`
}

function menuOptions(card: DependencyCard): DropdownOption[] {
  const options: DropdownOption[] = []
  // 两份都存在时，让用户选择用哪一份
  if (card.managedAvailable && card.systemAvailable) {
    options.push(
      { label: card.source === 'managed' ? '✓ 使用启动器副本' : '使用启动器副本', key: 'use-managed' },
      { label: card.source === 'system' ? '✓ 使用系统版本' : '使用系统版本', key: 'use-system' }
    )
  }
  if (card.source === 'managed') {
    // 重装对话框里已经能选安装位置，不再单独提供“更改安装位置”
    options.push({ label: '重装', key: 'reinstall' })
  } else if (!card.managedAvailable) {
    // 系统自带且还没有启动器副本时，提供“安装独立副本”
    options.push({ label: '安装独立副本', key: 'takeover' })
  }
  return options
}

function onMenu(card: DependencyCard, key: string | number) {
  if (key === 'reinstall') requestReinstall(card)
  else if (key === 'takeover') openTakeover(card)
  else if (key === 'use-managed') switchSource(card, false)
  else if (key === 'use-system') switchSource(card, true)
}

async function switchSource(card: DependencyCard, useSystem: boolean) {
  try {
    await appStore.setToolSource(card.key, useSystem)
    message.success(`${card.name} 已切换为${useSystem ? '系统版本' : '启动器副本'}`)
  } catch (e: any) {
    message.error(e?.message || '切换失败')
  }
}

async function doInstall(card: DependencyCard, force: boolean, verb?: string) {
  if (busyKey.value || botRunning.value || appStore.isBusy) return
  busyKey.value = card.key
  try {
    await appStore.installOne(card.key, force)
    message.success(`${card.name} ${verb ?? (force ? '重装' : '安装')}完成`)
  } catch (e: any) {
    message.error(e?.message || '安装失败')
  } finally {
    busyKey.value = null
  }
}

function installMissing(card: DependencyCard) {
  doInstall(card, false)
}

function requestReinstall(card: DependencyCard) {
  openInstall(card, 'reinstall')
}

function openTakeover(card: DependencyCard) {
  openInstall(card, 'takeover')
}

function openInstall(card: DependencyCard, mode: InstallMode) {
  installTarget.value = card
  // 重装时预填当前位置，方便直接改；安装独立副本默认留空用默认位置
  installPath.value =
    mode === 'reinstall' ? card.path || defaultDependencyPath(card.key) : ''
  installMode.value = mode
  installVisible.value = true
}

async function confirmInstall() {
  const card = installTarget.value
  if (!card) return
  try {
    // 先保存安装位置（留空即恢复默认位置），再重装/安装
    await appStore.setToolDir(card.key, installPath.value.trim())
  } catch (e: any) {
    message.error(e?.message || '保存安装位置失败')
    return
  }
  const mode = installMode.value
  installVisible.value = false
  installTarget.value = null
  await doInstall(card, true, mode === 'reinstall' ? '重装' : '安装')
}

async function refresh() {
  refreshing.value = true
  try {
    await appStore.refreshAll()
    message.success('状态已刷新')
  } finally {
    refreshing.value = false
  }
}

</script>

<style scoped>
.tool-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: 12px;
}

.path-line {
  font-family: 'JetBrains Mono', 'Cascadia Code', Consolas, monospace;
  font-size: 12px;
  line-height: 1.5;
  word-break: break-all;
}
</style>
