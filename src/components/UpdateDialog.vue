<template>
  <n-modal v-model:show="appStore.updateDialogVisible" preset="card" title="发现新版本" style="width: 480px">
    <n-space vertical :size="10">
      <n-text depth="3">
        当前版本 v{{ appStore.availableUpdate?.currentVersion }} → 新版本 v{{ appStore.availableUpdate?.version }}
      </n-text>
      <TaskProgress boxed :kinds="launcherKinds" />
      <n-scrollbar style="max-height: 240px">
        <n-text v-if="appStore.availableUpdate?.notes" style="white-space: pre-wrap">
          {{ appStore.availableUpdate.notes }}
        </n-text>
        <n-text v-else depth="3">（该版本没有提供更新说明）</n-text>
      </n-scrollbar>
    </n-space>
    <template #footer>
      <n-space justify="end" :size="8">
        <n-button size="small" quaternary :disabled="installing" @click="appStore.updateDialogVisible = false">
          稍后
        </n-button>
        <n-button size="small" type="primary" :loading="installing" @click="confirmInstall">
          立即更新
        </n-button>
      </n-space>
    </template>
  </n-modal>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { NButton, NModal, NScrollbar, NSpace, NText, useMessage } from 'naive-ui'
import { useAppStore } from '../stores/app'
import TaskProgress from './TaskProgress.vue'

// 启动器自更新用独立的 kind，进度只显示在本弹窗里
const launcherKinds = ['launcher']

const appStore = useAppStore()
const message = useMessage()
const installing = ref(false)

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
    appStore.updateDialogVisible = false
  }
}
</script>
