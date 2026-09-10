<template>
  <n-config-provider :theme="appStore.isDark ? darkTheme : null" :theme-overrides="activeOverrides" :locale="zhCN"
    :date-locale="dateZhCN" style="display: block; height: 100%">
    <n-message-provider>
      <n-dialog-provider>
        <n-layout has-sider style="height: 100vh">
          <n-layout-sider bordered :width="226" native-scrollbar>
            <AppSidebar />
          </n-layout-sider>
          <n-layout>
            <div style="height: 100%; display: flex; flex-direction: column; padding: 16px; box-sizing: border-box">
              <div style="flex: 1; min-height: 0; overflow: auto">
                <router-view />
              </div>
              <MainActionBar />
            </div>
          </n-layout>
        </n-layout>
        <UpdateDialog />
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>

<script setup lang="ts">
import { computed, onMounted } from 'vue'
import {
  NConfigProvider,
  NDialogProvider,
  NLayout,
  NLayoutSider,
  NMessageProvider,
  darkTheme,
  dateZhCN,
  zhCN
} from 'naive-ui'
import AppSidebar from './components/AppSidebar.vue'
import MainActionBar from './components/MainActionBar.vue'
import UpdateDialog from './components/UpdateDialog.vue'
import { useAppStore } from './stores/app'
import { darkThemeOverrides, lightThemeOverrides } from './theme'

const appStore = useAppStore()

const activeOverrides = computed(() => (appStore.isDark ? darkThemeOverrides : lightThemeOverrides))

onMounted(async () => {
  if (appStore.mode === 'checking') {
    await appStore.initListeners()
    await appStore.refreshAll()
  }
  appStore.scheduleAutoCheck()
})
</script>
