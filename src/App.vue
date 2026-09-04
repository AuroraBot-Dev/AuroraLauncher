<template>
  <n-config-provider :theme="isDark ? darkTheme : null" :theme-overrides="activeOverrides" :locale="zhCN"
    :date-locale="dateZhCN" style="display: block; height: 100%">
    <n-message-provider>
      <n-dialog-provider>
        <n-layout has-sider style="height: 100vh">
          <n-layout-sider bordered :width="226" native-scrollbar>
            <AppSidebar :is-dark="isDark" @toggle-theme="toggleTheme" />
          </n-layout-sider>
          <n-layout>
            <n-layout-content content-style="padding: 16px">
              <router-view />
              <MainActionBar />
            </n-layout-content>
          </n-layout>
        </n-layout>
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import {
  NConfigProvider,
  NDialogProvider,
  NLayout,
  NLayoutContent,
  NLayoutSider,
  NMessageProvider,
  darkTheme,
  dateZhCN,
  zhCN
} from 'naive-ui'
import AppSidebar from './components/AppSidebar.vue'
import MainActionBar from './components/MainActionBar.vue'
import { useAppStore } from './stores/app'
import { darkThemeOverrides, lightThemeOverrides } from './theme'

const appStore = useAppStore()

const preferredTheme = ref<'dark' | 'light'>(
  (localStorage.getItem('aurora-theme') as 'dark' | 'light') || 'dark'
)
const isDark = computed(() => preferredTheme.value === 'dark')
const activeOverrides = computed(() => (isDark.value ? darkThemeOverrides : lightThemeOverrides))

function toggleTheme() {
  preferredTheme.value = isDark.value ? 'light' : 'dark'
  localStorage.setItem('aurora-theme', preferredTheme.value)
}

onMounted(() => {
  if (appStore.mode === 'checking') {
    appStore.initListeners().then(() => appStore.refreshAll())
  }
})
</script>
