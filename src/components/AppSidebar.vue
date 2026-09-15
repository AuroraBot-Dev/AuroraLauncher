<template>
  <n-flex vertical style="height: 100%">
    <div style="flex: 1; min-height: 0; overflow-y: auto; padding: 8px">
      <div class="nav-item" :class="{ active: route.name === 'dashboard' }" @click="go('dashboard')">
        <n-icon :component="HomeFilled" :size="18" />
        <n-text style="flex: 1">主页</n-text>
      </div>

      <div class="nav-item" :class="{ active: route.name === 'chat' }" @click="go('chat')">
        <n-icon :component="ChatDotRound" :size="18" />
        <n-text style="flex: 1">对话</n-text>
      </div>

      <div class="nav-item" :class="{ active: route.name === 'environment' }" @click="go('environment')">
        <n-icon :component="Cpu" :size="18" />
        <n-text style="flex: 1">运行环境</n-text>
        <n-tag size="small" :type="readyCount === 4 ? 'info' : 'warning'" :bordered="false">
          {{ readyCount }}/4
        </n-tag>
      </div>

      <div class="nav-item" :class="{ active: route.name === 'kernel' }" @click="go('kernel')">
        <n-icon :component="Connection" :size="18" />
        <n-text style="flex: 1">内核</n-text>
        <n-tag size="small" :type="appStore.kernelStatus.exists ? 'info' : 'warning'" :bordered="false">
          {{ appStore.kernelStatus.exists ? '已就绪' : '未下载' }}
        </n-tag>
      </div>

      <div class="nav-item" :class="{ active: route.name === 'config' }" @click="go('config')">
        <n-icon :component="Operation" :size="18" />
        <n-text style="flex: 1">配置</n-text>
      </div>
    </div>

    <n-divider style="margin: 0" />

    <n-flex vertical :size="2" style="flex: none; padding: 8px">
      <div class="nav-item" :class="{ active: route.name === 'settings' }" @click="go('settings')">
        <n-icon :component="Tools" :size="18" />
        <n-text style="flex: 1">设置</n-text>
      </div>
      <div class="nav-item" :class="{ active: route.name === 'about' }" @click="go('about')">
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
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { NDivider, NFlex, NIcon, NTag, NText } from 'naive-ui'
import { ChatDotRound, Connection, Cpu, HomeFilled, InfoFilled, Operation, Tools } from '@element-plus/icons-vue'
import { useAppStore } from '../stores/app'

const appStore = useAppStore()
const route = useRoute()
const router = useRouter()

const readyCount = computed(
  () =>
    [
      appStore.dependencyStatus.git,
      appStore.dependencyStatus.uv,
      appStore.dependencyStatus.python,
      appStore.dependencyStatus.pnpm
    ].filter((tool) => tool.installed).length
)

function go(name: string) {
  if (route.name === name) return
  router.push({ name })
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

.nav-item.active {
  background-color: rgba(128, 128, 128, 0.18);
}
</style>
