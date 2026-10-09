<script setup lang="ts">
// 命令抽屉：**由下往上**滑出，用 xterm 显示「跑了什么命令 + 它的输出」。
//
// 为什么要它：中继是命令行程序，出了问题（装服务失败、端口占用）真正的原因都在它的输出里。
// 界面只给一句「失败了」时，用户只能猜 —— 抽屉把原始输出留在手边，可复制、可贴给维护者。
//
// 参考 antfu/node-modules-inspector 的做法：把 CLI 输出当一等公民展示，
// 用 `@xterm/xterm` 渲染（等宽、支持 ANSI 颜色），`@xterm/addon-fit` 跟随容器尺寸。

import { FitAddon } from '@xterm/addon-fit'
import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'

import { onCommandEvent } from '../ipc'
import type { CommandEvent } from '../types'
import { drawer } from '../locales/zh-CN'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ (event: 'toggle', open: boolean): void }>()

const host = ref<HTMLDivElement | null>(null)
const terminal = ref<Terminal | null>(null)
const fitAddon = new FitAddon()
const lineCount = ref(0)

/** 记一次调用：命令、输出、退出码都进同一个终端。 */
function write(event: CommandEvent) {
  const term = terminal.value
  if (!term) return

  switch (event.kind) {
    case 'command':
      term.write(`\r\n\x1b[36m$ ${event.line}\x1b[0m\r\n`)
      break
    case 'stderr':
      term.write(`\x1b[31m${event.line}\x1b[0m\r\n`)
      break
    case 'exit':
      term.write(`\x1b[90m—— 退出码 ${event.line} ——\x1b[0m\r\n`)
      break
    default:
      term.write(`${event.line}\r\n`)
  }

  lineCount.value += 1
}

async function fit() {
  // 抽屉关着时容器高度是 0，此时 fit() 会算出 0 列 —— 所以只在打开后量
  if (!props.open) return
  await nextTick()
  try {
    fitAddon.fit()
  } catch {
    // 容器还没布局好（极早期的一次调用）：下一次 resize 会补上
  }
}

function clear() {
  terminal.value?.clear()
  lineCount.value = 0
}

function copy() {
  const term = terminal.value
  if (!term) return
  // xterm 6 的 IBuffer 只有 getLine（没有 getLines），所以自己走一遍
  const buffer = term.buffer.active
  const lines: string[] = []
  for (let y = 0; y < buffer.length; y += 1) {
    lines.push(buffer.getLine(y)?.translateToString(true) ?? '')
  }
  void navigator.clipboard.writeText(lines.join('\n').trimEnd())
}

let stopListening: (() => void) | null = null

onMounted(async () => {
  const term = new Terminal({
    convertEol: false,
    cursorBlink: false,
    disableStdin: true, // 这是日志抽屉，不是交互式 shell
    fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
    fontSize: 12,
    scrollback: 5000,
    theme: { background: '#0b0f14', foreground: '#d7dee8', cursor: '#0b0f14' },
  })
  term.loadAddon(fitAddon)
  term.open(host.value!)
  terminal.value = term
  await fit()

  // 命令事件由 Rust 侧发（每次中继调用都会发一串）
  stopListening = await onCommandEvent(write)

  window.addEventListener('resize', fit)
})

onBeforeUnmount(() => {
  window.removeEventListener('resize', fit)
  stopListening?.()
  terminal.value?.dispose()
})

watch(() => props.open, fit)
</script>

<template>
  <section
    class="drawer"
    :class="open ? 'drawer-open' : 'drawer-closed'"
    :aria-hidden="!open"
    data-testid="command-drawer"
  >
    <header class="drawer-header">
      <button type="button" class="drawer-handle" @click="emit('toggle', !open)">
        <span class="drawer-title">{{ drawer.title }}</span>
        <span class="drawer-hint">{{ open ? drawer.hintOpen : drawer.hintClosed }}</span>
      </button>

      <div class="drawer-actions">
        <button type="button" class="drawer-action" @click="clear">{{ drawer.clear }}</button>
        <button type="button" class="drawer-action" @click="copy">{{ drawer.copy }}</button>
        <button type="button" class="drawer-action" @click="emit('toggle', false)">
          {{ drawer.close }}
        </button>
      </div>
    </header>

    <div ref="host" class="drawer-term" />
  </section>
</template>

<style scoped>
.drawer {
  position: fixed;
  right: 0;
  bottom: 0;
  left: 0;
  z-index: 30;
  display: flex;
  flex-direction: column;
  height: 45vh;
  background: #0b0f14;
  border-top: 1px solid #1f2937;
  box-shadow: 0 -12px 32px rgb(0 0 0 / 45%);
  transition: transform 220ms ease;
}

.drawer-closed {
  transform: translateY(100%);
  /* 关闭时不可点，否则会挡住底下的按钮 */
  pointer-events: none;
}

.drawer-open {
  transform: translateY(0);
}

.drawer-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 6px 12px;
  border-bottom: 1px solid #1f2937;
}

.drawer-handle {
  display: flex;
  flex: 1;
  align-items: baseline;
  gap: 10px;
  padding: 0;
  color: #d7dee8;
  font: inherit;
  text-align: left;
  cursor: pointer;
  background: none;
  border: 0;
}

.drawer-title {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
}

.drawer-hint {
  color: #6b7280;
  font-size: 11px;
}

.drawer-actions {
  display: flex;
  gap: 6px;
}

.drawer-action {
  padding: 3px 8px;
  color: #9ca3af;
  font: inherit;
  font-size: 11px;
  cursor: pointer;
  background: none;
  border: 1px solid #1f2937;
  border-radius: 4px;
}

.drawer-action:hover {
  color: #e5e7eb;
  border-color: #374151;
}

.drawer-term {
  flex: 1;
  min-height: 0;
  padding: 6px 8px;
}
</style>
