<template>
  <n-layout-footer bordered style="
      position: fixed;
      left: 226px;
      right: 0;
      bottom: 0;
      z-index: 10;
      padding: 12px 16px;
    ">
    <n-flex align="center" justify="end" :size="16">
      <n-flex v-if="progressVisible" align="center" :size="12" style="flex: 1; min-width: 0">
        <n-text depth="3" style="white-space: nowrap">{{ progressLabel }}</n-text>
        <n-progress type="line" :percentage="percent" :processing="indeterminate" :show-indicator="false" :height="8"
          style="flex: 1" />
        <n-text v-if="!indeterminate" depth="3">{{ percent }}%</n-text>
      </n-flex>

      <n-button :disabled="appStore.isBusy" :loading="checking || installing" quaternary @click="checkForUpdates">
        检查更新
      </n-button>
      <n-button size="large" type="primary" :disabled="appStore.isBusy" :loading="appStore.coreDownloading"
        style="min-width: 112px" @click="primaryAction">
        <template #icon>
          <n-icon :component="actionIcon" :size="20" />
        </template>
        {{ actionLabel }}
      </n-button>
    </n-flex>

    <n-modal v-model:show="updateDialogVisible" preset="card" title="发现新版本" style="width: 480px">
      <n-space vertical :size="10">
        <n-text depth="3">
          当前版本 v{{ availableUpdate?.currentVersion }} → 新版本 v{{ availableUpdate?.version }}
        </n-text>
        <n-scrollbar style="max-height: 240px">
          <n-text v-if="availableUpdate?.notes" style="white-space: pre-wrap">{{ availableUpdate.notes }}</n-text>
          <n-text v-else depth="3">（该版本没有提供更新说明）</n-text>
        </n-scrollbar>
      </n-space>
      <template #footer>
        <n-space justify="end" :size="8">
          <n-button size="small" quaternary :disabled="installing" @click="updateDialogVisible = false">
            稍后
          </n-button>
          <n-button size="small" type="primary" :loading="installing" @click="confirmInstall">
            立即更新
          </n-button>
        </n-space>
      </template>
    </n-modal>
  </n-layout-footer>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import {
  NButton,
  NFlex,
  NIcon,
  NLayoutFooter,
  NModal,
  NProgress,
  NScrollbar,
  NSpace,
  NText,
  useMessage
} from 'naive-ui'
import { CircleClose, Download, VideoPlay } from '@element-plus/icons-vue'
import { useAppStore } from '../stores/app'
import type { LauncherUpdate } from '../types'

const appStore = useAppStore()
const message = useMessage()

const checking = ref(false)
const installing = ref(false)
const updateDialogVisible = ref(false)
const availableUpdate = ref<LauncherUpdate | null>(null)

// 启动后延迟片刻自动检查一次更新；会话内只检查一次，避免切换页面反复弹窗
let autoCheckRan = false
const AUTO_CHECK_DELAY_MS = 1500

onMounted(() => {
  if (autoCheckRan) return
  autoCheckRan = true
  window.setTimeout(runAutoCheck, AUTO_CHECK_DELAY_MS)
})

const botRunning = computed(() => appStore.process?.running)
const kernelReady = computed(() => appStore.kernelStatus.exists)
const actionLabel = computed(() => {
  if (botRunning.value) return '停止'
  if (!kernelReady.value) return appStore.coreDownloading ? '下载中' : '下载核心'
  return '启动'
})
const actionIcon = computed(() =>
  botRunning.value ? CircleClose : kernelReady.value ? VideoPlay : Download
)
const percent = computed(() => {
  const p = appStore.progress
  if (!p?.total) return 0
  return Math.min(100, Math.round((p.current / p.total) * 100))
})
const progressLabel = computed(() => appStore.progress?.label || '正在处理…')
const progressVisible = computed(() => appStore.progress != null)
const indeterminate = computed(() => !appStore.progress?.total)

async function primaryAction() {
  if (appStore.isBusy) return
  if (botRunning.value) {
    await stopBot()
    return
  }
  if (!kernelReady.value) {
    await downloadCore()
    return
  }
  await launchBot()
}

async function stopBot() {
  try {
    await appStore.stopBot()
    message.success('AuroraBot 已停止')
  } catch (e: any) {
    message.error(e?.message || '停止失败')
  }
}

async function downloadCore() {
  if (appStore.coreDownloading) return
  appStore.setCoreDownloading(true)
  try {
    await appStore.cloneOrUpdate()
    if (kernelReady.value) message.success('AuroraBot 核心下载完成')
    else message.error('AuroraBot 核心尚未就绪')
  } catch (e: any) {
    message.error(e?.message || '核心下载失败')
  } finally {
    appStore.setCoreDownloading(false)
  }
}

async function launchBot() {
  try {
    await appStore.startBot(true)
    message.success('AuroraBot 已启动')
  } catch (e: any) {
    message.error(e?.message || '启动失败')
  }
}

async function checkForUpdates() {
  if (checking.value || installing.value || appStore.isBusy) return
  checking.value = true
  try {
    const update = await appStore.checkForUpdates()
    if (update) {
      availableUpdate.value = update
      updateDialogVisible.value = true
    } else {
      message.info('当前已是最新版本')
    }
  } catch (e: any) {
    message.error(e?.message || '检查更新失败')
  } finally {
    checking.value = false
  }
}

async function runAutoCheck() {
  // 启动初期若有任务（如首次状态刷新）占用，则延迟重试，保证每个会话都自动检查一次
  if (appStore.isBusy || checking.value || installing.value) {
    window.setTimeout(runAutoCheck, AUTO_CHECK_DELAY_MS)
    return
  }
  try {
    // 自动检查失败不打扰用户：详情已写入日志，仅在有新版本时弹窗
    const update = await appStore.checkForUpdates()
    if (update) {
      availableUpdate.value = update
      updateDialogVisible.value = true
    }
  } catch {
    // 忽略（网络不可用 / 更新源暂无发布等），保持静默
  }
}

async function confirmInstall() {
  if (installing.value) return
  installing.value = true
  try {
    await appStore.installUpdate()
    message.success('更新已安装，应用将自动重启')
  } catch (e: any) {
    message.error(e?.message || '安装更新失败')
  } finally {
    installing.value = false
    updateDialogVisible.value = false
  }
}
</script>
