<template>
  <n-drawer :show="show" :width="380" placement="right" :auto-focus="false" @update:show="emit('update:show', $event)">
    <n-drawer-content title="设置" :native-scrollbar="false">
      <n-flex vertical :size="18">
        <n-card size="small" title="外观">
          <n-flex align="center" justify="space-between">
            <n-text>主题</n-text>
            <n-radio-group :value="appStore.themeMode" size="small" @update:value="onThemeChange">
              <n-radio-button value="light">
                浅色
              </n-radio-button>
              <n-radio-button value="dark">
                深色
              </n-radio-button>
              <n-radio-button value="system">
                跟随系统
              </n-radio-button>
            </n-radio-group>
          </n-flex>
        </n-card>

        <n-card size="small" title="更新">
          <n-flex vertical :size="10">
            <n-flex align="center" justify="space-between">
              <n-text>启动时自动检查更新</n-text>
              <n-switch :value="appStore.autoCheckUpdate" @update:value="onAutoCheckChange" />
            </n-flex>
            <n-text depth="3" style="font-size: 12px">
              关闭后仍可在“关于”中手动检查更新。
            </n-text>
          </n-flex>
        </n-card>

        <n-card size="small" title="运行">
          <n-flex vertical :size="8">
            <n-button text type="primary" style="justify-content: flex-start" @click="refresh">
              刷新状态
            </n-button>
            <n-button text type="primary" style="justify-content: flex-start" @click="emit('open-logs')">
              查看运行日志
            </n-button>
            <n-button text type="primary" style="justify-content: flex-start" @click="appStore.openFolder()">
              打开运行时目录
            </n-button>
          </n-flex>
        </n-card>
      </n-flex>
    </n-drawer-content>
  </n-drawer>
</template>

<script setup lang="ts">
import { NButton, NCard, NDrawer, NDrawerContent, NFlex, NRadioButton, NRadioGroup, NSwitch, NText, useMessage } from 'naive-ui'
import { useAppStore } from '../stores/app'
import type { ThemeMode } from '../types'

defineProps<{
  show: boolean
}>()

const emit = defineEmits<{
  'update:show': [value: boolean]
  'open-logs': []
}>()

const appStore = useAppStore()
const message = useMessage()

function onAutoCheckChange(value: boolean) {
  appStore.setAutoCheckUpdate(value)
  message.success(value ? '已开启启动时自动检查更新' : '已关闭启动时自动检查更新')
}

function onThemeChange(value: string | number) {
  appStore.setThemeMode(value as ThemeMode)
}

async function refresh() {
  await appStore.refreshAll()
  message.success('状态已刷新')
}
</script>
