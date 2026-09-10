<template>
  <div style="flex: none; padding-top: 12px">
    <n-flex align="center" justify="end" :size="16">
      <n-flex v-if="progressVisible" align="center" :size="12" style="flex: 1; min-width: 0">
        <n-text depth="3" style="white-space: nowrap">{{ progressLabel }}</n-text>
        <n-progress type="line" :percentage="percent" :processing="indeterminate" :show-indicator="false" :height="8"
          style="flex: 1" />
        <n-text v-if="!indeterminate" depth="3">{{ percent }}%</n-text>
      </n-flex>

      <n-button size="large" type="primary" :disabled="appStore.isBusy" :loading="appStore.coreDownloading"
        style="min-width: 112px" @click="primaryAction">
        <template #icon>
          <n-icon :component="actionIcon" :size="20" />
        </template>
        {{ actionLabel }}
      </n-button>
    </n-flex>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { NButton, NFlex, NIcon, NProgress, NText, useMessage } from 'naive-ui'
import { CircleClose, Download, VideoPlay } from '@element-plus/icons-vue'
import { useAppStore } from '../stores/app'

const appStore = useAppStore()
const message = useMessage()

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
    await handleDownloadCore()
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

async function handleDownloadCore() {
  if (appStore.coreDownloading) return
  try {
    await appStore.downloadCore()
    if (kernelReady.value) message.success('AuroraBot 核心下载完成')
    else message.error('AuroraBot 核心尚未就绪')
  } catch (e: any) {
    message.error(e?.message || '核心下载失败')
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
</script>
