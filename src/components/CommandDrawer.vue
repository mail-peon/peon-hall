<script setup lang="ts">
// 命令抽屉：**常驻底部**的一条 footer（终端图标），面板**由下往上**展开，
// 用 xterm 显示「跑了什么命令 + 它的输出」，跑命令时**自动弹出**。
//
// 为什么要它：中继是命令行程序，出了问题（装服务失败、端口被占）真正的原因都在它的输出里。
// 界面只给一句「失败了」时用户只能猜 —— 抽屉把原始输出留在手边，可复制、可贴给维护者。
//
// **不自动弹出**：只在用户点底部那条 footer 时展开（轮询、安装、卸载都不打扰）。
// 事件里的 origin（user/auto）留着做元数据，将来要按来源过滤就靠它。
//
// 参考 antfu/node-modules-inspector：把 CLI 输出当一等公民展示。
// `@xterm/xterm` 渲染（等宽 + ANSI 颜色），`@xterm/addon-fit` 跟随容器尺寸。

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

/** 把一行事件写进终端（命令、输出、退出码都进同一个视图）。 */
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
  // 关着的时候容器高度是 0，量出来是 0 列
  if (!props.open) return
  await nextTick()
  try {
    fitAddon.fit()
  } catch {
    // 容器还没布局好：动画结束后会再量一次
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
const timers: number[] = []

onMounted(async () => {
  const term = new Terminal({
    convertEol: false,
    cursorBlink: false,
    disableStdin: true, // 这是日志抽屉，不是交互式 shell
    fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
    fontSize: 12,
    scrollback: 5000,
    // 终端自身透明：毛玻璃由外层 .drawer 负责，这样整块面板才是同一个材质
    theme: { background: 'rgba(0, 0, 0, 0)', foreground: '#e5e7eb', cursor: '#0b0f14' },
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
  timers.forEach((timer) => window.clearTimeout(timer))
})

watch(
  () => props.open,
  async (open) => {
    await fit()
    if (open) {
      // 展开动画（220ms）结束后再量一次，否则列数按动画中途的高度算
      timers.push(window.setTimeout(() => void fit(), 240))
    }
  },
)
</script>

<template>
  <footer class="drawer" data-testid="command-drawer">
    <!-- 毛玻璃遮罩：展开时压住底下的内容（点它收起），让终端成为焦点 -->
    <div
      v-if="open"
      class="drawer-scrim"
      data-testid="command-drawer-scrim"
      @click="emit('toggle', false)"
    />

    <!-- 面板：高度 0 ↔ 90%，视觉上就是从 footer 由下往上抽出来 -->
    <section class="drawer-panel" :class="{ 'drawer-panel-open': open }" :aria-hidden="!open">
      <header class="drawer-header">
        <span class="drawer-title">{{ drawer.title }}</span>
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

    <!-- 常驻的 footer 条：任何时候都在，点它开合 -->
    <button
      type="button"
      class="drawer-bar"
      data-testid="command-drawer-toggle"
      :aria-expanded="open"
      @click="emit('toggle', !open)"
    >
      <svg class="drawer-icon" viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M2.5 3.5 6 7 2.5 10.5M7.5 11.5h6"
          fill="none"
          stroke="currentColor"
          stroke-width="1.4"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
      <span class="drawer-label">{{ drawer.title }}</span>
      <span v-if="lineCount > 0" class="drawer-count">{{ drawer.lines(lineCount) }}</span>
      <span class="drawer-hint">{{ open ? drawer.hintOpen : drawer.hintClosed }}</span>
    </button>
  </footer>
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
  /* 整块面板是毛玻璃：终端文字浮在模糊的界面之上，而不是「玻璃上贴一块黑板」 */
  background: rgb(11 15 20 / 72%);
  backdrop-filter: blur(18px) saturate(130%);
  border-top: 1px solid rgb(255 255 255 / 8%);
}

.drawer-panel {
  display: flex;
  flex-direction: column;
  height: 0;
  overflow: hidden;
  transition: height 220ms ease;
}

.drawer-panel-open {
  /* 100% 减掉常驻 footer 条的高度（别把它顶出屏幕） */
  height: calc(100vh - 32px);
}

.drawer-scrim {
  position: fixed;
  inset: 0;
  z-index: -1;
  background: rgb(3 7 12 / 45%);
  backdrop-filter: blur(10px) saturate(120%);
  cursor: pointer;
}

.drawer-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 6px 12px;
  border-bottom: 1px solid #1f2937;
}

.drawer-title {
  color: #d7dee8;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
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

.drawer-bar {
  display: flex;
  flex: none;
  gap: 8px;
  align-items: center;
  height: 32px;
  padding: 0 12px;
  color: #9ca3af;
  font: inherit;
  font-size: 12px;
  text-align: left;
  cursor: pointer;
  background: rgb(17 24 39 / 55%);
  border: 0;
}

.drawer-bar:hover {
  color: #e5e7eb;
  background: #1f2937;
}

.drawer-icon {
  flex: none;
  width: 14px;
  height: 14px;
}

.drawer-label {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}

.drawer-count {
  padding: 1px 6px;
  color: #6b7280;
  font-size: 11px;
  background: #1f2937;
  border-radius: 999px;
}

.drawer-hint {
  margin-left: auto;
  color: #4b5563;
  font-size: 11px;
}
</style>
