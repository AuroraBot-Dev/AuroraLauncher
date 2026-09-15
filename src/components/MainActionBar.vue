<template>
  <div style="flex: none; padding-top: 12px">
    <n-flex align="center" justify="end" :size="16">
      <TaskProgress :kinds="lifecycleKinds" style="flex: 1; min-width: 0" />

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
import { NButton, NFlex, NIcon, useMessage } from 'naive-ui'
import { CircleClose, Download, VideoPlay } from '@element-plus/icons-vue'
import { useAppStore } from '../stores/app'
import TaskProgress from './TaskProgress.vue'

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
// 这条进度只跟内核/启动相关：下载核心（clone）、更新内核（update）、启动（sync）。
// 依赖安装（git/uv/python/pnpm）有自己的进度条，不在这里动。
const lifecycleKinds = ['clone', 'update', 'sync']

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
    await appStore.startBot()
    message.success('AuroraBot 已启动')
  } catch (e: any) {
    message.error(e?.message || '启动失败')
  }
}
</script>
