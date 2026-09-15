<template>
  <n-flex vertical :size="18">
    <n-flex vertical align="center" :size="8" style="padding: 8px 0">
      <n-text strong style="font-size: 20px">AuroraLauncher</n-text>
      <n-tag size="small" :bordered="false">v{{ version || 'dev' }}</n-tag>
      <n-text depth="3" style="font-size: 12px; text-align: center; line-height: 1.6">
        AuroraBot 自包含启动器：自主管理 Python/uv/Git/pnpm 与 AuroraBot 内核，运行时与宿主环境隔离。
      </n-text>
    </n-flex>

    <n-flex :size="8">
      <n-button style="flex: 1" :loading="appStore.checkingUpdate" @click="checkUpdate">
        检查更新
      </n-button>
      <n-button style="flex: 1" @click="copyDiagnostics">复制诊断信息</n-button>
    </n-flex>

    <n-collapse>
      <n-collapse-item title="组件版本" name="versions">
        <n-descriptions :column="1" label-placement="left" size="small">
          <n-descriptions-item label="内核">{{ kernelVersion }}</n-descriptions-item>
          <n-descriptions-item label="Python">{{ depVersion(appStore.dependencyStatus.python) }}</n-descriptions-item>
          <n-descriptions-item label="uv">{{ depVersion(appStore.dependencyStatus.uv) }}</n-descriptions-item>
          <n-descriptions-item label="Git">{{ depVersion(appStore.dependencyStatus.git) }}</n-descriptions-item>
          <n-descriptions-item label="pnpm">{{ depVersion(appStore.dependencyStatus.pnpm) }}</n-descriptions-item>
        </n-descriptions>
      </n-collapse-item>

      <n-collapse-item title="系统信息" name="system">
        <n-descriptions :column="1" label-placement="left" size="small">
          <n-descriptions-item label="系统">{{ osLabel }}</n-descriptions-item>
          <n-descriptions-item label="运行时目录">
            <n-text style="word-break: break-all">{{ appStore.runtimeInfo?.root || '—' }}</n-text>
          </n-descriptions-item>
        </n-descriptions>
      </n-collapse-item>

      <n-collapse-item title="社区与支持" name="community">
        <n-flex vertical :size="8">
          <n-button text type="primary" style="justify-content: flex-start" @click="open(LINKS.repo)">
            GitHub 仓库
          </n-button>
          <n-button text type="primary" style="justify-content: flex-start" @click="open(LINKS.docs)">
            文档站（含平台接入说明）
          </n-button>
          <n-button text type="primary" style="justify-content: flex-start" @click="open(LINKS.issues)">
            问题反馈（Issues）
          </n-button>
          <n-button text type="primary" style="justify-content: flex-start" @click="open(LINKS.releases)">
            版本发布（Releases）
          </n-button>
        </n-flex>
      </n-collapse-item>

      <n-collapse-item title="法律与许可" name="legal">
        <n-flex vertical :size="10">
          <n-text style="font-size: 12px">
            本项目基于 MIT 许可发布。
            <n-button text type="primary" style="font-size: 12px" @click="open(LINKS.license)">
              查看完整许可
            </n-button>
          </n-text>
          <n-text depth="3" style="font-size: 12px; line-height: 1.7">
            隐私说明：不收集任何个人信息，运行数据均保存在本地（默认 <n-text code>~/.aurora-launcher/tool</n-text>，便携版在程序同级 <n-text code>tool/</n-text>）；仅在检查更新时访问 GitHub Releases。
          </n-text>
        </n-flex>
      </n-collapse-item>
    </n-collapse>

    <n-text depth="3" style="font-size: 12px; text-align: center">
      © 2026 JuFireX | AuroraBot-Dev
    </n-text>
  </n-flex>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import {
  NButton,
  NCollapse,
  NCollapseItem,
  NDescriptions,
  NDescriptionsItem,
  NFlex,
  NTag,
  NText,
  useMessage
} from 'naive-ui'
import { getVersion } from '@tauri-apps/api/app'
import { useAppStore } from '../stores/app'
import { LINKS } from '../links'
import { formatToolVersion } from '../format'
import type { ToolMeta } from '../types'

const appStore = useAppStore()
const message = useMessage()
const version = ref('')

onMounted(async () => {
  try {
    version.value = await getVersion()
  } catch {
    version.value = 'dev'
  }
})

const osLabel = computed(() => {
  const info = appStore.runtimeInfo
  if (!info) return '—'
  const os = ({ windows: 'Windows', macos: 'macOS', linux: 'Linux' } as Record<string, string>)[info.os] ?? info.os
  return `${os} · ${info.arch}`
})

const kernelVersion = computed(() =>
  appStore.kernelStatus.exists ? appStore.kernelStatus.commitShort || '已就绪' : '未下载'
)

function depVersion(meta: ToolMeta): string {
  return formatToolVersion(meta.version, meta.installed)
}

async function checkUpdate() {
  if (appStore.isBusy) {
    message.warning('正在处理其他任务，请稍后再试')
    return
  }
  const update = await appStore.requestUpdateCheck()
  if (!update) message.info('当前已是最新版本')
}

function diagnosticsText(): string {
  const dep = appStore.dependencyStatus
  return [
    `AuroraLauncher v${version.value || 'dev'}`,
    `系统: ${osLabel.value}`,
    `运行时目录: ${appStore.runtimeInfo?.root || '—'}`,
    `内核: ${kernelVersion.value}`,
    `Python: ${depVersion(dep.python)}`,
    `uv: ${depVersion(dep.uv)}`,
    `Git: ${depVersion(dep.git)}`,
    `pnpm: ${depVersion(dep.pnpm)}`
  ].join('\n')
}

async function copyDiagnostics() {
  const text = diagnosticsText()
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    const area = document.createElement('textarea')
    area.value = text
    document.body.appendChild(area)
    area.select()
    document.execCommand('copy')
    document.body.removeChild(area)
  }
  message.success('诊断信息已复制')
}

function open(url: string) {
  appStore.openExternal(url)
}
</script>
