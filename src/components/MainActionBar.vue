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
      <n-flex v-if="appStore.coreDownloading" align="center" :size="12" style="flex: 1; min-width: 0">
        <n-text depth="3" style="white-space: nowrap">{{ progressLabel }}</n-text>
        <n-progress type="line" :percentage="percent" :processing="!appStore.progress?.total" :show-indicator="false"
          :height="8" style="flex: 1" />
        <n-text depth="3">{{ percent }}%</n-text>
      </n-flex>

      <n-button size="large" type="primary" :disabled="botRunning" :loading="appStore.isBusy" style="min-width: 112px"
        @click="primaryAction">
        <template #icon>
          <n-icon :component="kernelReady ? VideoPlay : Download" :size="20" />
        </template>
        {{ actionLabel }}
      </n-button>
    </n-flex>
  </n-layout-footer>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import {
  NButton,
  NFlex,
  NIcon,
  NLayoutFooter,
  NProgress,
  NText,
  useMessage
} from 'naive-ui'
import { Download, VideoPlay } from '@element-plus/icons-vue'
import { useAppStore } from '../stores/app'

const appStore = useAppStore()
const message = useMessage()

const botRunning = computed(() => appStore.process?.running)
const kernelReady = computed(() => appStore.kernelStatus.exists)
const actionLabel = computed(() => {
  if (botRunning.value) return '运行中'
  if (!kernelReady.value) return appStore.coreDownloading ? '下载中' : '下载核心'
  return '启动'
})
const percent = computed(() => {
  const p = appStore.progress
  if (!p?.total) return 0
  return Math.min(100, Math.round((p.current / p.total) * 100))
})
const progressLabel = computed(() => appStore.progress?.label || '正在下载 AuroraBot 核心')

async function primaryAction() {
  if (botRunning.value || appStore.isBusy) return
  if (!kernelReady.value) {
    await downloadCore()
    return
  }
  await launchBot()
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
</script>
