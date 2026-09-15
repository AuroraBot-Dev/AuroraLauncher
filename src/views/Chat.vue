<template>
  <n-flex vertical :size="12" style="height: 100%">
    <n-flex align="center" justify="space-between" :wrap="false">
      <n-flex vertical :size="2">
        <n-text strong style="font-size: 18px">对话</n-text>
        <n-text depth="3" style="font-size: 12px">
          直接和 Bot 对话；输入会作为 console.input 进入她的世界线。
        </n-text>
      </n-flex>
      <n-space :size="8" :wrap="false">
        <n-tag v-if="!running" size="small" type="warning" :bordered="false">未运行</n-tag>
        <n-button size="small" :disabled="lines.length === 0" @click="appStore.clearChat()">清空</n-button>
      </n-space>
    </n-flex>

    <div ref="outputRef" class="chat-log">
      <div v-for="(line, index) in lines" :key="index">
        <div v-if="line.role === 'user'" class="row user">
          <div class="bubble user-bubble">{{ line.text }}</div>
        </div>
        <div v-else-if="line.role === 'bot'" class="row bot">
          <div class="bubble bot-bubble">{{ line.text }}</div>
        </div>
        <div v-else class="row system">
          <div class="system-text">{{ line.text }}</div>
        </div>
      </div>
      <n-empty v-if="lines.length === 0" description="点「启动」后开始对话" style="margin-top: 40px" />
    </div>

    <n-flex :size="8" :wrap="false" style="flex: none">
      <n-input v-model:value="draft" :disabled="!running" type="textarea"
        :autosize="{ minRows: 1, maxRows: 4 }"
        :placeholder="running ? '输入消息，回车发送（Shift+Enter 换行；可先用 /help）' : 'Bot 未运行，请先在主页启动'"
        @keydown="onKeydown" />
      <n-button type="primary" :disabled="!running || !draft.trim()" @click="send">发送</n-button>
    </n-flex>
  </n-flex>
</template>

<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { NButton, NEmpty, NFlex, NInput, NSpace, NTag, NText, useMessage } from 'naive-ui'
import { useAppStore } from '../stores/app'

const appStore = useAppStore()
const message = useMessage()

const draft = ref('')
const outputRef = ref<HTMLElement | null>(null)

const lines = computed(() => appStore.chatLines)
const running = computed(() => appStore.process?.running ?? false)

async function scrollToBottom() {
  await nextTick()
  const el = outputRef.value
  if (el) el.scrollTop = el.scrollHeight
}

watch(lines, scrollToBottom, { deep: true })

function onKeydown(event: KeyboardEvent) {
  if (event.key !== 'Enter' || event.shiftKey) return
  event.preventDefault()
  send()
}

async function send() {
  if (!running.value || !draft.value.trim()) return
  const text = draft.value
  draft.value = ''
  try {
    await appStore.sendChatInput(text)
    await scrollToBottom()
  } catch (e: any) {
    message.error(e?.message || '发送失败')
  }
}
</script>

<style scoped>
.chat-log {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 12px;
  border-radius: 7px;
  background-color: rgba(128, 128, 128, 0.08);
  font-size: 13px;
  line-height: 1.6;
}

.row {
  display: flex;
  margin-bottom: 8px;
}

.row.user {
  justify-content: flex-end;
}

.row.bot {
  justify-content: flex-start;
}

.bubble {
  max-width: 78%;
  padding: 7px 11px;
  border-radius: 9px;
  white-space: pre-wrap;
  word-break: break-word;
}

.user-bubble {
  background-color: #006be6;
  color: #ffffff;
}

.bot-bubble {
  background-color: rgba(128, 128, 128, 0.18);
}

.system-text {
  font-family: 'JetBrains Mono', 'Cascadia Code', Consolas, monospace;
  font-size: 12px;
  color: #8497a4;
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
