<template>
  <n-flex vertical :size="12" style="height: 100%">
    <n-flex align="center" justify="space-between" :wrap="false">
      <n-flex vertical :size="2">
        <n-text strong style="font-size: 18px">运行日志</n-text>
        <n-text depth="3" style="font-size: 12px">安装、内核更新与 Bot 运行的实时输出。</n-text>
      </n-flex>
      <n-button size="small" quaternary :disabled="appStore.logs.length === 0" @click="appStore.clearLogs()">
        清空
      </n-button>
    </n-flex>

    <n-scrollbar style="flex: 1; min-height: 0">
      <div v-for="(line, index) in appStore.logs" :key="index" class="log-line">
        <n-text depth="3">{{ line.time }}</n-text>
        <n-text :type="levelType(line.level)" style="margin-left: 8px">{{ line.message }}</n-text>
      </div>
      <n-empty v-if="appStore.logs.length === 0" description="暂无日志" style="margin-top: 40px" />
    </n-scrollbar>
  </n-flex>
</template>

<script setup lang="ts">
import { NButton, NEmpty, NFlex, NScrollbar, NText } from 'naive-ui'
import { useAppStore } from '../stores/app'
import type { LogLine } from '../types'

const appStore = useAppStore()

function levelType(level: LogLine['level']): 'default' | 'success' | 'warning' | 'error' {
  if (level === 'success') return 'success'
  if (level === 'warn') return 'warning'
  if (level === 'error') return 'error'
  return 'default'
}
</script>

<style scoped>
.log-line {
  padding: 3px 2px;
  font-family: 'JetBrains Mono', 'Cascadia Code', Consolas, monospace;
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-all;
}
</style>
