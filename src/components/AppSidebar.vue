<template>
  <n-flex vertical style="height: 100%">
    <div style="flex: 1; min-height: 0; overflow-y: auto; padding: 8px">
      <div class="nav-item" @click="envDrawerVisible = true">
        <n-icon :component="Cpu" :size="18" />
        <n-text style="flex: 1">运行环境</n-text>
        <n-tag size="small" :type="readyCount === 4 ? 'info' : 'warning'" :bordered="false">
          {{ readyCount }}/4
        </n-tag>
      </div>

      <div class="nav-item" @click="kernelDrawerVisible = true">
        <n-icon :component="Connection" :size="18" />
        <n-text style="flex: 1">内核</n-text>
        <n-tag size="small" :type="appStore.kernelStatus.exists ? 'info' : 'warning'" :bordered="false">
          {{ appStore.kernelStatus.exists ? '已就绪' : '未下载' }}
        </n-tag>
      </div>
    </div>

    <n-divider style="margin: 0" />

    <n-flex vertical :size="2" style="flex: none; padding: 8px">
      <div class="nav-item" @click="settingsDrawerVisible = true">
        <n-icon :component="Tools" :size="18" />
        <n-text style="flex: 1">设置</n-text>
      </div>
      <div class="nav-item" @click="aboutDrawerVisible = true">
        <n-icon :component="InfoFilled" :size="18" />
        <n-text style="flex: 1">关于</n-text>
      </div>
    </n-flex>

    <n-flex v-if="appStore.mode === 'preview'" justify="center" style="flex: none; padding: 0 8px 10px">
      <n-tag type="warning" size="small" :bordered="false" round>
        演示模式
      </n-tag>
    </n-flex>
  </n-flex>

  <n-drawer v-model:show="envDrawerVisible" :width="380" placement="right" :auto-focus="false">
    <n-drawer-content :native-scrollbar="false">
      <template #header>
        <n-flex align="center" justify="space-between" style="width: 100%">
          <n-text strong>运行环境</n-text>
          <n-tag :type="readyCount === 4 ? 'info' : 'warning'" size="small" :bordered="false">
            {{ readyCount }}/4 就绪
          </n-tag>
        </n-flex>
      </template>
      <n-flex vertical :size="10">
        <n-card v-for="card in dependencyCards" :key="card.key" size="small" :title="card.name">
          <template #header-extra>
            <n-tag :type="sourceTagType(card.source)" size="small" :bordered="false">
              {{ sourceLabel(card.source) }}
            </n-tag>
          </template>
          <n-flex align="center" justify="space-between" :wrap="false">
            <n-text depth="3"
              style="font-size: 12px; flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap">
              {{ formatToolVersion(card.version, card.installed) }}
            </n-text>
            <n-space :size="6" :wrap="false">
              <n-button size="tiny" style="width: 56px" @click="openPathDialog(card)">
                定位
              </n-button>
              <n-button size="tiny" :type="card.source === 'managed' ? 'default' : 'primary'" style="width: 56px"
                :loading="busyKey === card.key"
                :disabled="botRunning || appStore.isBusy || (busyKey !== null && busyKey !== card.key)"
                @click="requestInstall(card)">
                {{ card.source === 'managed' ? '重装' : '安装' }}
              </n-button>
            </n-space>
          </n-flex>
        </n-card>
      </n-flex>
    </n-drawer-content>
  </n-drawer>

  <n-drawer v-model:show="kernelDrawerVisible" :width="380" placement="right" :auto-focus="false">
    <n-drawer-content title="内核" :native-scrollbar="false">
      <n-flex vertical :size="14">
        <n-descriptions :column="1" label-placement="left" size="small" bordered>
          <n-descriptions-item label="状态">
            {{ appStore.kernelStatus.exists ? '已就绪' : '未下载' }}
          </n-descriptions-item>
          <n-descriptions-item label="提交">
            {{ appStore.kernelStatus.commitShort || '—' }}
          </n-descriptions-item>
          <n-descriptions-item label="日期">
            {{ appStore.kernelStatus.date || '—' }}
          </n-descriptions-item>
          <n-descriptions-item label="说明">
            {{ appStore.kernelStatus.message || '—' }}
          </n-descriptions-item>
        </n-descriptions>
        <n-button block type="primary" :disabled="botRunning || appStore.isBusy" :loading="appStore.coreDownloading"
          @click="updateKernel">
          {{ appStore.kernelStatus.exists ? '更新内核' : '下载核心' }}
        </n-button>
      </n-flex>
    </n-drawer-content>
  </n-drawer>

  <n-drawer v-model:show="logDrawerVisible" :width="560" placement="right" :auto-focus="false">
    <n-drawer-content :native-scrollbar="false">
      <template #header>
        <n-flex align="center" justify="space-between" style="width: 100%">
          <n-text strong>运行日志</n-text>
          <n-button size="tiny" quaternary :disabled="appStore.logs.length === 0" @click="clearLogs">
            清空
          </n-button>
        </n-flex>
      </template>
      <n-scrollbar style="height: 100%">
        <div v-for="(line, index) in appStore.logs" :key="index" class="log-line">
          <n-text depth="3">{{ line.time }}</n-text>
          <n-text :type="levelType(line.level)" style="margin-left: 8px">{{ line.message }}</n-text>
        </div>
        <n-empty v-if="appStore.logs.length === 0" description="暂无日志" style="margin-top: 40px" />
      </n-scrollbar>
    </n-drawer-content>
  </n-drawer>

  <n-modal v-model:show="pathDialogVisible" preset="card"
    :title="pathTarget ? `自定义 ${pathTarget.name} 安装路径` : '自定义安装路径'" style="width: 440px">
    <n-space vertical :size="8">
      <n-text depth="3">输入该依赖的安装目录，留空则使用默认路径。修改后需重新安装才会生效。</n-text>
      <n-input v-model:value="pathDraft" :placeholder="pathTarget ? defaultDependencyPath(pathTarget.key) : ''"
        clearable />
    </n-space>
    <template #footer>
      <n-space justify="end" :size="8">
        <n-button size="small" quaternary @click="cancelPathDialog">
          取消
        </n-button>
        <n-button size="small" type="primary" @click="confirmPathDialog">
          保存
        </n-button>
      </n-space>
    </template>
  </n-modal>

  <SettingsDrawer v-model:show="settingsDrawerVisible" @open-logs="openLogs" />
  <AboutDrawer v-model:show="aboutDrawerVisible" />
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import {
  NButton,
  NCard,
  NDescriptions,
  NDescriptionsItem,
  NDrawer,
  NDrawerContent,
  NDivider,
  NEmpty,
  NFlex,
  NIcon,
  NInput,
  NModal,
  NScrollbar,
  NSpace,
  NTag,
  NText,
  useDialog,
  useMessage
} from 'naive-ui'
import { Connection, Cpu, InfoFilled, Tools } from '@element-plus/icons-vue'
import { useAppStore } from '../stores/app'
import { formatToolVersion } from '../format'
import SettingsDrawer from './SettingsDrawer.vue'
import AboutDrawer from './AboutDrawer.vue'
import type { LogLine, ToolKind, ToolSource } from '../types'

const appStore = useAppStore()
const message = useMessage()
const dialog = useDialog()
const busyKey = ref<ToolKind | null>(null)
const botRunning = computed(() => appStore.process?.running ?? false)

const envDrawerVisible = ref(false)
const kernelDrawerVisible = ref(false)
const logDrawerVisible = ref(false)
const settingsDrawerVisible = ref(false)
const aboutDrawerVisible = ref(false)

const pathTarget = ref<DependencyCard | null>(null)
const pathDraft = ref('')
const pathDialogVisible = ref(false)

function defaultDependencyPath(kind: ToolKind) {
  return `runtime/tools/${kind}`
}

interface DependencyCard {
  key: ToolKind
  name: string
  installed: boolean
  source: ToolSource
  version: string
  path: string
}

const dependencyCards = computed<DependencyCard[]>(() => [
  {
    key: 'git',
    name: 'Git',
    installed: appStore.dependencyStatus.git.installed,
    source: appStore.dependencyStatus.git.source,
    version: appStore.dependencyStatus.git.version,
    path: appStore.dependencyStatus.git.path
  },
  {
    key: 'uv',
    name: 'uv',
    installed: appStore.dependencyStatus.uv.installed,
    source: appStore.dependencyStatus.uv.source,
    version: appStore.dependencyStatus.uv.version,
    path: appStore.dependencyStatus.uv.path
  },
  {
    key: 'python',
    name: 'Python',
    installed: appStore.dependencyStatus.python.installed,
    source: appStore.dependencyStatus.python.source,
    version: appStore.dependencyStatus.python.version,
    path: appStore.dependencyStatus.python.path
  },
  {
    key: 'pnpm',
    name: 'pnpm',
    installed: appStore.dependencyStatus.pnpm.installed,
    source: appStore.dependencyStatus.pnpm.source,
    version: appStore.dependencyStatus.pnpm.version,
    path: appStore.dependencyStatus.pnpm.path
  }
])

const readyCount = computed(() => dependencyCards.value.filter((card) => card.installed).length)

function levelType(level: LogLine['level']): 'default' | 'success' | 'warning' | 'error' {
  if (level === 'success') return 'success'
  if (level === 'warn') return 'warning'
  if (level === 'error') return 'error'
  return 'default'
}

function sourceLabel(source: ToolSource): string {
  if (source === 'managed') return '就绪'
  if (source === 'system') return '系统'
  return '缺失'
}

function sourceTagType(source: ToolSource): 'info' | 'warning' {
  if (source === 'missing') return 'warning'
  return 'info'
}

function openLogs() {
  settingsDrawerVisible.value = false
  logDrawerVisible.value = true
}

function clearLogs() {
  appStore.logs = []
}

async function doInstall(card: DependencyCard, force: boolean) {
  if (busyKey.value || botRunning.value || appStore.isBusy) return
  busyKey.value = card.key
  try {
    await appStore.installOne(card.key, force)
    message.success(`${card.name} ${force ? '重装' : '安装'}完成`)
  } catch (e: any) {
    message.error(e?.message || '安装失败')
  } finally {
    busyKey.value = null
  }
}

/// 按来源决定安装行为：
/// - 系统已装：询问是否在默认路径重新装一份自管副本
/// - 未安装：询问是否安装
/// - 自管已装：询问是否重装
function requestInstall(card: DependencyCard) {
  if (busyKey.value || botRunning.value || appStore.isBusy) return
  if (card.source === 'system') {
    dialog.info({
      title: `检测到系统已安装 ${card.name}`,
      content: `当前使用的是系统 PATH 上的 ${card.name} ${card.version}。是否在默认路径重新安装一份由启动器自管的副本？`,
      positiveText: '重新安装',
      negativeText: '使用系统的',
      onPositiveClick: () => doInstall(card, true)
    })
    return
  }
  if (card.source === 'managed') {
    dialog.warning({
      title: `重新安装 ${card.name}`,
      content: `将删除启动器自管的 ${card.name} 并重新下载安装，确定吗？`,
      positiveText: '重新安装',
      negativeText: '取消',
      onPositiveClick: () => doInstall(card, true)
    })
    return
  }
  dialog.info({
    title: `安装 ${card.name}`,
    content: `未检测到 ${card.name}，是否立即安装到默认路径？`,
    positiveText: '安装',
    negativeText: '取消',
    onPositiveClick: () => doInstall(card, false)
  })
}

async function updateKernel() {
  if (appStore.coreDownloading || appStore.isBusy || botRunning.value) return
  try {
    await appStore.downloadCore()
    message.success(appStore.kernelStatus.exists ? '内核更新完成' : '核心下载完成')
  } catch (e: any) {
    message.error(e?.message || '内核操作失败')
  }
}

function openPathDialog(card: DependencyCard) {
  pathTarget.value = card
  pathDraft.value = card.path || defaultDependencyPath(card.key)
  pathDialogVisible.value = true
}

function cancelPathDialog() {
  pathDialogVisible.value = false
  pathTarget.value = null
}

async function confirmPathDialog() {
  if (!pathTarget.value) return
  const target = pathTarget.value
  try {
    await appStore.setToolDir(target.key, pathDraft.value.trim())
    message.success(`${target.name} 路径已保存，重新安装后生效`)
    pathDialogVisible.value = false
    pathTarget.value = null
  } catch (e: any) {
    message.error(e?.message || '保存路径失败')
  }
}
</script>

<style scoped>
.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border-radius: 7px;
  cursor: pointer;
  transition: background-color 0.2s;
}

.nav-item:hover {
  background-color: rgba(128, 128, 128, 0.12);
}

.log-line {
  padding: 3px 2px;
  font-family: 'JetBrains Mono', 'Cascadia Code', Consolas, monospace;
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-all;
}
</style>
