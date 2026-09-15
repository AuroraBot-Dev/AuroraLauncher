<template>
  <n-flex v-if="progress" align="center" :size="12" :class="{ boxed }">
    <n-text depth="3" style="white-space: nowrap">{{ label }}</n-text>
    <n-progress type="line" :percentage="percent" :processing="indeterminate" :show-indicator="false" :height="8"
      style="flex: 1" />
    <n-text v-if="!indeterminate" depth="3">{{ percent }}%</n-text>
    <n-text v-if="speedLabel" depth="3" style="white-space: nowrap">{{ speedLabel }}</n-text>
  </n-flex>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { NFlex, NProgress, NText } from 'naive-ui'
import { useAppStore } from '../stores/app'
import { formatSpeed } from '../format'

const props = defineProps<{
  /// 只显示这些 kind 的任务：每个进度条只反映自己那一类，别的任务不会让它动
  kinds: string[]
  /// 页面内使用时加一层浅底，和卡片区分开；主页底部条用裸样式
  boxed?: boolean
}>()

const appStore = useAppStore()

// 当前属于本进度条的任务；没有任务、或 kind 不在白名单时为空
const progress = computed(() => {
  const current = appStore.progress
  if (!current) return null
  return props.kinds.includes(current.kind) ? current : null
})

const percent = computed(() => {
  const p = progress.value
  if (!p?.total) return 0
  return Math.min(100, Math.round((p.current / p.total) * 100))
})
const indeterminate = computed(() => !progress.value?.total)
const label = computed(() => progress.value?.label || '正在处理…')
const speedLabel = computed(() => formatSpeed(progress.value?.speed ?? 0))
</script>

<style scoped>
.boxed {
  padding: 8px 12px;
  border-radius: 7px;
  background-color: rgba(128, 128, 128, 0.08);
}
</style>
