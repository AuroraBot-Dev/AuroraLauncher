<template>
  <n-flex vertical :size="16">
    <n-flex align="center" justify="space-between" :wrap="false">
      <n-flex vertical :size="2">
        <n-text strong style="font-size: 18px">AuroraBot 内核</n-text>
        <n-text depth="3" style="font-size: 12px">
          内核以 Git 仓库形式更新，更新前会检查本地修改，避免覆盖你的改动。
        </n-text>
      </n-flex>
      <n-space :size="8" :wrap="false">
        <n-button size="small" :disabled="!kernelReady" @click="appStore.openKernelDir()">打开位置</n-button>
        <n-button size="small" :disabled="botRunning || appStore.isBusy" @click="initKernel">初始化</n-button>
        <n-button size="small" type="primary" :disabled="botRunning || appStore.isBusy"
          :loading="appStore.coreDownloading" @click="updateKernel">
          {{ kernelReady ? '更新内核' : '下载核心' }}
        </n-button>
      </n-space>
    </n-flex>

    <TaskProgress boxed :kinds="kernelKinds" />

    <n-card size="small" title="版本信息">
      <n-descriptions :column="1" label-placement="left" size="small" bordered>
        <n-descriptions-item label="状态">
          <n-tag size="small" :type="kernelReady ? 'info' : 'warning'" :bordered="false">
            {{ kernelReady ? '已就绪' : '未下载' }}
          </n-tag>
        </n-descriptions-item>
        <n-descriptions-item label="提交">{{ appStore.kernelStatus.commitShort || '—' }}</n-descriptions-item>
        <n-descriptions-item label="分支">{{ appStore.kernelStatus.branch || '—' }}</n-descriptions-item>
        <n-descriptions-item label="日期">{{ appStore.kernelStatus.date || '—' }}</n-descriptions-item>
        <n-descriptions-item label="说明">{{ appStore.kernelStatus.message || '—' }}</n-descriptions-item>
        <n-descriptions-item label="远端">
          <n-text style="word-break: break-all">{{ appStore.kernelStatus.remote || '—' }}</n-text>
        </n-descriptions-item>
      </n-descriptions>
    </n-card>

    <n-card size="small" title="内核描述">
      <template #header-extra>
        <n-button size="tiny" :disabled="!kernelReady" :loading="appStore.loadingKernelAbout" @click="loadAbout">
          重新获取
        </n-button>
      </template>
      <n-empty v-if="!appStore.kernelAbout" description="暂未获取到内核描述" style="padding: 16px 0" />
      <template v-else>
        <pre class="about-logo">{{ aboutParts.head }}</pre>
        <pre v-if="aboutParts.body" class="about-body">{{ aboutParts.body }}</pre>
      </template>
    </n-card>
  </n-flex>
</template>

<script setup lang="ts">
import { computed, onMounted, watch } from 'vue'
import {
  NButton,
  NCard,
  NDescriptions,
  NDescriptionsItem,
  NEmpty,
  NFlex,
  NSpace,
  NTag,
  NText,
  useDialog,
  useMessage
} from 'naive-ui'
import { useAppStore } from '../stores/app'
import TaskProgress from '../components/TaskProgress.vue'

const appStore = useAppStore()
const message = useMessage()
const dialog = useDialog()

// 本页只显示内核相关进度：克隆/更新（clone/update）与依赖同步（sync）；
// 工具安装的进度在「运行环境」页。
const kernelKinds = ['clone', 'update', 'sync']

const kernelReady = computed(() => appStore.kernelStatus.exists)
const botRunning = computed(() => appStore.process?.running ?? false)

/// `aurora about` 的第一段是半块字符拼的 LOGO，必须紧贴行距才完整；
/// 以第一个空行为界拆成 LOGO 和正文两段，分别设置行高。
const aboutParts = computed(() => {
  const text = appStore.kernelAbout ?? ''
  const split = text.indexOf('\n\n')
  if (split === -1) return { head: text, body: '' }
  return { head: text.slice(0, split), body: text.slice(split + 2) }
})

async function updateKernel() {
  if (appStore.coreDownloading || appStore.isBusy || botRunning.value) return
  try {
    await appStore.downloadCore()
    message.success(kernelReady.value ? '内核更新完成' : '核心下载完成')
  } catch (e: any) {
    message.error(e?.message || '内核操作失败')
  }
}

/// 初始化会在「虚拟环境来源与当前选择不一致」时重建 venv——那是几分钟的重操作
/// （要重新安装依赖），所以先弹框说清楚再让用户决定，而不是悄悄卡住。
function confirmVenvRebuild(): Promise<boolean> {
  return new Promise((resolve) => {
    dialog.warning({
      title: '将重建虚拟环境',
      content:
        '当前虚拟环境与所选的 Python 来源不一致，初始化会按当前来源重建它。工具和内核源码不受影响；venv 里的依赖会从本地缓存重新装入（通常不需要联网下载）。是否继续？',
      positiveText: '继续初始化',
      negativeText: '取消',
      onPositiveClick: () => resolve(true),
      onNegativeClick: () => resolve(false),
      onClose: () => resolve(false),
      onMaskClick: () => resolve(false)
    })
  })
}

async function initKernel() {
  if (appStore.dependencyStatus.pythonVenvStale && !(await confirmVenvRebuild())) return
  try {
    await appStore.runSetup()
    message.success('内核初始化完成')
  } catch (e: any) {
    message.error(e?.message || '初始化失败')
  }
}

async function loadAbout() {
  try {
    await appStore.loadKernelAbout()
  } catch (e: any) {
    message.error(e?.message || '获取内核描述失败')
  }
}

/// 内核就绪且尚未拿到描述时自动加载一次；失败静默（详情已写日志）。
function autoLoad() {
  if (!kernelReady.value || appStore.kernelAbout) return
  appStore.loadKernelAbout().catch(() => {})
}

onMounted(autoLoad)
watch(kernelReady, (ready) => {
  if (ready) autoLoad()
})
</script>

<style scoped>
.about-logo,
.about-body {
  margin: 0;
  font-family: 'JetBrains Mono', 'Cascadia Code', Consolas, monospace;
  font-size: 12px;
}

/* LOGO 用半块字符，行高必须为 1，否则上下两行会分离、看起来缺一块 */
.about-logo {
  line-height: 1;
  white-space: pre;
  /* 不要设 overflow：块字符的墨迹会超出行框，overflow 会裁掉最后一行 */
  padding: 2px 0;
}

.about-body {
  margin-top: 10px;
  line-height: 1.7;
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
