<template>
  <n-flex vertical :size="16">
    <n-flex align="center" justify="space-between" :wrap="false">
      <n-flex vertical :size="2">
        <n-text strong style="font-size: 18px">配置</n-text>
        <n-text depth="3" style="font-size: 12px">
          内核运行所需的密钥与平台开关；修改后重新启动 Bot 才会生效。
        </n-text>
      </n-flex>
      <n-space :size="8" :wrap="false">
        <n-button size="small" :disabled="!kernelReady" @click="appStore.openKernelDir()">
          打开配置目录
        </n-button>
        <n-button size="small" :loading="appStore.loadingConfig" :disabled="!kernelReady" @click="reload">
          刷新
        </n-button>
      </n-space>
    </n-flex>

    <n-alert v-if="!kernelReady" type="warning" :bordered="false">
      内核尚未下载，暂无配置可编辑。请先在「内核」页下载核心。
    </n-alert>
    <n-alert v-else-if="!configReady" type="warning" :bordered="false">
      内核已下载但尚未初始化，配置文件还没生成。请先在「内核」页点「初始化」。
    </n-alert>

    <template v-else>
      <n-card size="small" title="密钥">
        <n-empty v-if="envEntries.length === 0" description="无需填写的密钥" style="padding: 12px 0" />
        <n-flex v-else vertical :size="12">
          <n-select v-model:value="selectedKey" size="small" :options="keyOptions"
            placeholder="选择要填写的变量" />
          <n-flex v-if="currentEntry" vertical :size="6">
            <n-flex align="center" :size="8">
              <n-text depth="3" style="font-size: 12px">{{ currentEntry.name }}</n-text>
              <n-tag size="tiny" :bordered="false" :type="currentEntry.value ? 'success' : 'warning'">
                {{ currentEntry.value ? '已填写' : '未填写' }}
              </n-tag>
            </n-flex>
            <n-flex :size="8" :wrap="false">
              <n-input v-model:value="drafts[currentEntry.name]" size="small"
                :type="currentEntry.secret ? 'password' : 'text'"
                :show-password-on="currentEntry.secret ? 'click' : undefined"
                :placeholder="currentEntry.secret ? '输入密钥' : '输入值'" clearable />
              <n-button size="small" type="primary" :loading="savingKey === currentEntry.name"
                @click="saveEnv(currentEntry.name)">
                保存
              </n-button>
            </n-flex>
          </n-flex>
        </n-flex>
      </n-card>

      <n-card size="small" title="消息平台">
        <n-flex vertical :size="12">
          <n-empty v-if="apps.length === 0" description="没有可用的平台" style="padding: 12px 0" />
          <n-flex v-for="app in apps" :key="app.package" align="center" justify="space-between" :wrap="false">
            <n-text style="word-break: break-all">{{ app.package }}</n-text>
            <n-switch :value="app.enabled" :loading="savingApp === app.package"
              @update:value="(value: boolean) => toggleApp(app.package, value)" />
          </n-flex>
          <n-text depth="3" style="font-size: 12px">
            启用的平台会在下次启动 Bot 时连接；部分平台还需要填写对应的密钥（见上方）。
          </n-text>
        </n-flex>
      </n-card>
    </template>
  </n-flex>
</template>

<script setup lang="ts">
import { computed, h, onMounted, reactive, ref, watch } from 'vue'
import {
  NAlert,
  NButton,
  NCard,
  NEmpty,
  NFlex,
  NIcon,
  NInput,
  NSelect,
  NSpace,
  NSwitch,
  NTag,
  NText,
  useMessage
} from 'naive-ui'
import type { SelectOption } from 'naive-ui'
import { CloseBold } from '@element-plus/icons-vue'
import { useAppStore } from '../stores/app'

const appStore = useAppStore()
const message = useMessage()

const kernelReady = computed(() => appStore.kernelStatus.exists)
// 内核已 clone 但没跑过 setup 时配置文件还不存在，配置项会读成空
const configReady = computed(() => appStore.launcherConfig?.initialized ?? false)
const envEntries = computed(() => appStore.launcherConfig?.env ?? [])
const apps = computed(() => appStore.launcherConfig?.apps ?? [])

const drafts = reactive<Record<string, string>>({})
const selectedKey = ref<string | null>(null)
const savingKey = ref<string | null>(null)
const savingApp = ref<string | null>(null)

const keyOptions = computed<SelectOption[]>(() =>
  envEntries.value.map((entry) => ({
    label: entry.value ? `${entry.name}（已填写）` : entry.name,
    value: entry.name
  }))
)

const currentEntry = computed(
  () => envEntries.value.find((entry) => entry.name === selectedKey.value) ?? null
)

function syncDrafts() {
  for (const entry of envEntries.value) {
    drafts[entry.name] = entry.value
  }
  if (!envEntries.value.some((entry) => entry.name === selectedKey.value)) {
    selectedKey.value = envEntries.value[0]?.name ?? null
  }
}

async function reload() {
  if (!kernelReady.value) return
  try {
    await appStore.loadLauncherConfig()
    syncDrafts()
  } catch (e: any) {
    message.error(e?.message || '读取配置失败')
  }
}

async function saveEnv(name: string) {
  savingKey.value = name
  try {
    await appStore.setEnvValue(name, drafts[name] ?? '')
    await appStore.loadLauncherConfig()
    syncDrafts()
    message.success(`${name} 已保存`)
  } catch (e: any) {
    message.error(e?.message || '保存失败')
  } finally {
    savingKey.value = null
  }
}

async function toggleApp(pkg: string, enabled: boolean) {
  savingApp.value = pkg
  try {
    await appStore.setAppEnabled(pkg, enabled)
    await appStore.loadLauncherConfig()
    syncDrafts()
    if (enabled) {
      message.success(`已启用 ${pkg}`)
    } else {
      // 停用不是“成功”，用 × 而不是默认的 √
      message.warning(`已停用 ${pkg}`, {
        icon: () => h(NIcon, { component: CloseBold })
      })
    }
  } catch (e: any) {
    message.error(e?.message || '切换失败')
  } finally {
    savingApp.value = null
  }
}

onMounted(reload)
watch(kernelReady, (ready) => {
  if (ready) reload()
})
// 内核更新（提交号变化）后配置可能新增密钥/应用，自动重新读取
watch(
  () => appStore.kernelStatus.commitShort,
  (commit) => {
    if (commit) reload()
  }
)
</script>
