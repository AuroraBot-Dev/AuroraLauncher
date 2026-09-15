<template>
  <!-- 主页保持极简：只给一句状态提示，操作按钮在底部操作条上 -->
  <n-flex vertical align="center" justify="center" :size="8" style="height: 100%; padding-bottom: 32px">
    <n-text strong style="font-size: 20px">AuroraLauncher</n-text>
    <n-text depth="3" style="font-size: 13px; text-align: center; line-height: 1.7">
      {{ hint }}
    </n-text>
  </n-flex>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { NFlex, NText } from 'naive-ui'
import { useAppStore } from '../stores/app'

const appStore = useAppStore()

const hint = computed(() => {
  if (!appStore.kernelStatus.exists) {
    return '还没有下载内核，点下面的「下载核心」开始。'
  }
  if (appStore.process?.running) {
    return `AuroraBot 正在运行（pid ${appStore.process.pid}），可在「对话」页和她交互。`
  }
  return '内核已就绪，点下面的「启动」运行 AuroraBot；首次启动会先同步依赖。'
})
</script>
