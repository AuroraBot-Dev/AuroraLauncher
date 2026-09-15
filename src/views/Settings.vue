<template>
  <n-flex vertical :size="16">
    <n-flex vertical :size="2">
      <n-text strong style="font-size: 18px">设置</n-text>
      <n-text depth="3" style="font-size: 12px">外观、更新与运行时操作。</n-text>
    </n-flex>

    <n-card size="small" title="外观">
      <n-flex align="center" justify="space-between">
        <n-text>主题</n-text>
        <n-radio-group :value="appStore.themeMode" size="small" @update:value="onThemeChange">
          <n-radio-button value="light">浅色</n-radio-button>
          <n-radio-button value="dark">深色</n-radio-button>
          <n-radio-button value="system">跟随系统</n-radio-button>
        </n-radio-group>
      </n-flex>
    </n-card>

    <n-card size="small" title="更新">
      <n-flex vertical :size="10">
        <n-flex align="center" justify="space-between">
          <n-text>启动时自动检查更新</n-text>
          <n-switch :value="appStore.autoCheckUpdate" @update:value="onAutoCheckChange" />
        </n-flex>
        <n-text depth="3" style="font-size: 12px">关闭后仍可在“关于”中手动检查更新。</n-text>
      </n-flex>
    </n-card>

    <n-card size="small" title="网络">
      <n-flex vertical :size="12">
        <n-flex align="center" justify="space-between" :wrap="false">
          <n-text>包下载源</n-text>
          <n-radio-group :value="appStore.downloadSource" size="small" @update:value="onDownloadSourceChange">
            <n-radio-button value="mirror">国内镜像</n-radio-button>
            <n-radio-button value="official">官方源</n-radio-button>
          </n-radio-group>
        </n-flex>
        <n-text depth="3" style="font-size: 12px">
          内核安装 Python 依赖时使用的源；国内镜像更快，官方源用于镜像不可用时。
        </n-text>

        <n-divider style="margin: 0" />

        <n-text>GitHub 加速前缀</n-text>
        <n-text depth="3" style="font-size: 12px">
          工具安装包（Git / uv / pnpm）与 Python 解释器默认从 github.com 下载，国内可能很慢。
          填入加速前缀后改从它下载，留空则直连。只影响 GitHub，不影响上面的包下载源。
        </n-text>
        <n-flex :size="8" :wrap="false">
          <n-input v-model:value="githubMirror" placeholder="https://ghproxy.net/（留空 = 直连）" clearable
            @keyup.enter="saveGithubMirror" />
          <n-button size="small" type="primary" :loading="savingMirror" @click="saveGithubMirror">
            保存
          </n-button>
        </n-flex>
        <n-text type="warning" style="font-size: 12px">
          加速地址属第三方服务，请自行确认可信。
        </n-text>
      </n-flex>
    </n-card>

    <n-card size="small" title="运行">
      <n-flex vertical :size="8">
        <n-button text type="primary" style="justify-content: flex-start" @click="refresh">
          刷新状态
        </n-button>
        <n-button text type="primary" style="justify-content: flex-start" @click="router.push({ name: 'logs' })">
          查看运行日志
        </n-button>
        <n-button text type="primary" style="justify-content: flex-start" @click="appStore.openFolder()">
          打开运行时目录
        </n-button>
      </n-flex>
    </n-card>
  </n-flex>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import {
  NButton,
  NCard,
  NDivider,
  NFlex,
  NInput,
  NRadioButton,
  NRadioGroup,
  NSwitch,
  NText,
  useMessage
} from 'naive-ui'
import { useRouter } from 'vue-router'
import { useAppStore } from '../stores/app'
import type { ThemeMode } from '../types'

const appStore = useAppStore()
const message = useMessage()
const router = useRouter()

// 本地编辑副本：只在点「保存」时提交，避免边输边改
const githubMirror = ref(appStore.githubMirror)
watch(
  () => appStore.githubMirror,
  (value) => {
    githubMirror.value = value
  }
)
const savingMirror = ref(false)

async function saveGithubMirror() {
  savingMirror.value = true
  try {
    await appStore.setGithubMirror(githubMirror.value)
    message.success('已保存')
  } catch (e: any) {
    message.error(e?.message || '保存失败')
  } finally {
    savingMirror.value = false
  }
}

function onAutoCheckChange(value: boolean) {
  appStore.setAutoCheckUpdate(value)
  message.success(value ? '已开启启动时自动检查更新' : '已关闭启动时自动检查更新')
}

function onThemeChange(value: string | number) {
  appStore.setThemeMode(value as ThemeMode)
}

async function onDownloadSourceChange(value: string | number) {
  const source = value as string
  try {
    await appStore.setDownloadSource(source)
    message.success(source === 'official' ? '已切换到官方源' : '已切换到国内镜像')
  } catch (e: any) {
    message.error(e?.message || '切换失败')
  }
}

async function refresh() {
  await appStore.refreshAll()
  message.success('状态已刷新')
}
</script>
