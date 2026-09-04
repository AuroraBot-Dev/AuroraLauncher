<template>
  <n-space vertical :size="8">
    <n-progress :percentage="percentage" :processing="indeterminate" :show-indicator="!indeterminate" :height="8" />
    <n-text v-if="label" depth="2">{{ label }}</n-text>
  </n-space>
</template>

<script setup lang="ts">
import { NProgress, NSpace, NText } from 'naive-ui'

defineProps<{
  percentage: number
  indeterminate: boolean
  label?: string
}>()
</script>
