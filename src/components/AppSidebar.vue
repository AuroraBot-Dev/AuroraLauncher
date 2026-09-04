<template>
  <n-flex vertical style="height: 100%">
    <n-space vertical :size="6" style="
        flex: 1;
        min-height: 0;
        overflow-y: auto;
        padding: 8px 10px;
      ">
      <n-card v-for="card in dependencyCards" :key="card.key" :title="card.name" size="small">
        <template #header-extra>
          <n-tag :type="card.installed ? 'success' : 'warning'" size="small" :bordered="false">
            {{ card.installed ? '就绪' : '缺失' }}
          </n-tag>
        </template>
        <template #footer>
          <n-space align="center" justify="space-between" style="width: 100%">
            <n-button size="tiny" @click="openPathDialog(card)">
              定位
            </n-button>
            <n-button :type="card.installed ? 'default' : 'primary'" size="tiny" style="width: 68px"
              :loading="busyKey === card.key" :disabled="busyKey !== null && busyKey !== card.key"
              @click="installDependency(card)">
              {{ card.installed ? '重装' : '安装' }}
            </n-button>
          </n-space>
        </template>
      </n-card>
    </n-space>

    <n-space :size="4" style="flex: none; padding: 8px 10px">
      <n-tooltip trigger="hover">
        <template #trigger>
          <n-button quaternary circle size="small" aria-label="刷新状态" @click="refresh">
            <template #icon>
              <n-icon :component="Refresh" :size="16" />
            </template>
          </n-button>
        </template>
        刷新状态
      </n-tooltip>
      <n-tooltip trigger="hover">
        <template #trigger>
          <n-button quaternary circle size="small" :aria-label="isDark ? '切换浅色' : '切换深色'" @click="emit('toggleTheme')">
            <template #icon>
              <n-icon :component="isDark ? Sunny : Moon" :size="16" />
            </template>
          </n-button>
        </template>
        {{ isDark ? '切换浅色' : '切换深色' }}
      </n-tooltip>
      <n-tag v-if="appStore.mode === 'preview'" type="warning" size="small" :bordered="false" round>
        演示模式
      </n-tag>
    </n-space>
  </n-flex>

  <n-modal v-model:show="pathDialogVisible" preset="card"
    :title="pathTarget ? `自定义 ${pathTarget.name} 安装路径` : '自定义安装路径'" style="width: 440px">
    <n-space vertical :size="8">
      <n-text depth="3">输入该依赖的安装目录，留空则使用默认路径。</n-text>
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
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import {
  NButton,
  NCard,
  NFlex,
  NIcon,
  NInput,
  NModal,
  NSpace,
  NTag,
  NText,
  NTooltip,
  useMessage
} from 'naive-ui'
import { Moon, Refresh, Sunny } from '@element-plus/icons-vue'
import { useAppStore } from '../stores/app'
import type { ToolKind } from '../types'

defineProps<{
  isDark: boolean
}>()

const emit = defineEmits<{
  toggleTheme: []
}>()

const appStore = useAppStore()
const message = useMessage()
const busyKey = ref<ToolKind | null>(null)

const CUSTOM_PATH_STORAGE_KEY = 'aurora-custom-tool-paths'
const customPaths = ref<Record<ToolKind, string>>(loadCustomPaths())
const pathTarget = ref<DependencyCard | null>(null)
const pathDraft = ref('')
const pathDialogVisible = ref(false)

function defaultDependencyPath(kind: ToolKind) {
  return `runtime/tools/${kind}`
}

function loadCustomPaths(): Record<ToolKind, string> {
  const defaults: Record<ToolKind, string> = { git: '', uv: '', python: '', pnpm: '' }
  try {
    const saved = JSON.parse(localStorage.getItem(CUSTOM_PATH_STORAGE_KEY) || '{}')
    for (const kind of Object.keys(defaults) as ToolKind[]) {
      if (typeof saved[kind] === 'string') defaults[kind] = saved[kind]
    }
  } catch {
    // ignore malformed local storage
  }
  return defaults
}

interface DependencyCard {
  key: ToolKind
  name: string
  installed: boolean
}

const dependencyCards = computed<DependencyCard[]>(() => [
  {
    key: 'git',
    name: 'Git',
    installed: appStore.dependencyStatus.git.installed
  },
  {
    key: 'uv',
    name: 'uv',
    installed: appStore.dependencyStatus.uv.installed
  },
  {
    key: 'python',
    name: 'Python',
    installed: appStore.dependencyStatus.python.installed
  },
  {
    key: 'pnpm',
    name: 'pnpm',
    installed: appStore.dependencyStatus.pnpm.installed
  }
])

async function refresh() {
  await appStore.refreshAll()
}

async function installDependency(card: DependencyCard) {
  if (busyKey.value) return
  busyKey.value = card.key
  try {
    await appStore.installOne(card.key)
    message.success(`${card.name} 安装完成`)
  } catch (e: any) {
    message.error(e?.message || '安装失败')
  } finally {
    busyKey.value = null
  }
}

function openPathDialog(card: DependencyCard) {
  pathTarget.value = card
  pathDraft.value = customPaths.value[card.key] || defaultDependencyPath(card.key)
  pathDialogVisible.value = true
}

function cancelPathDialog() {
  pathDialogVisible.value = false
  pathTarget.value = null
}

function confirmPathDialog() {
  if (!pathTarget.value) return
  const target = pathTarget.value
  customPaths.value[target.key] = pathDraft.value.trim() || defaultDependencyPath(target.key)
  localStorage.setItem(CUSTOM_PATH_STORAGE_KEY, JSON.stringify(customPaths.value))
  message.success(`${target.name} 路径已保存`)
  pathDialogVisible.value = false
  pathTarget.value = null
}
</script>
