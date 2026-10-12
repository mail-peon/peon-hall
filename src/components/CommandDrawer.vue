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
import { SearchAddon } from '@xterm/addon-search'
import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { Button as AButton, Input as AInput, Space as ASpace } from 'ant-design-vue'
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'

import { onCommandEvent, copyText } from '../ipc'
import type { CommandEvent } from '../types'
import { drawer } from '../locales/zh-CN'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ (event: 'toggle', open: boolean): void }>()

const host = ref<HTMLDivElement | null>(null)
const terminal = ref<Terminal | null>(null)
const fitAddon = new FitAddon()
let searchAddon: SearchAddon | null = null
const lineCount = ref(0)
/** 搜索框可见性（Ctrl/Cmd+F 触发）。 */
const searchVisible = ref(false)
/** 搜索关键词。 */
const searchQuery = ref('')
/** 「找不到」的高亮 —— 用 antd 输入框的 status 控制。 */
const searchNotFound = ref(false)

/** 复制按钮的瞬时状态（成功/失败都要让用户看见，H-6）。 */
const copyState = ref<'idle' | 'copied' | 'failed'>('idle')
/** 状态显示多久。 */
const COPY_STATE_MS = 1500

/** 把一行事件写进终端（命令、输出、退出码都进同一个视图）。 */
function write(event: CommandEvent) {
  const term = terminal.value
  if (!term) return

  /*
   * 过滤 `auto` 起源的事件（10 秒一次的 `burrow service status --json` 轮询）：
   *   它**不是**用户操作，调试时也没人会翻 5000 行 service status 输出找问题。
   *   想看「中继现在在不在」直接看卡片上的状态徽标就够了 —— 命令抽屉是给人
   *   看「我刚刚按的那个按钮为什么没反应」的，auto 那两个不该来占位。
   *
   * 注释里的 4 行代价 = 挡掉用户每次打开抽屉都会看到「这 50 条 `burrow service status`
   * 都是哪冒出来的？」的困惑。
   */
  if (event.origin === 'auto')
    return

  /*
   * 时间戳：每行命令前面都带 `[HH:MM:SS]`（灰色）。
   *
   * 为什么用前端时间而不是 Rust 发的：
   * - Rust 那边只发「事件 id + 行内容」，多一个字段要改协议类型 + 三个 crate；
   * - 命令事件的发送节奏（用户点按钮 → Rust spawn → stdout 流）已经决定了
   *   一切以「用户视角**为主」的连续性 —— 一条命令下的所有行天然是同一秒级连续，
   *   用户能看到「这一坨是几点几分开始的」就够定位问题了。
   *
   * 这条黑框存在时：滚动 x5000 行找 5 分钟前那次失败的安装，就能用 `find` 跳。
   */
  const stamp = `\x1b[90m[${formatTimestamp(new Date())}]\x1b[0m `
  switch (event.kind) {
    case 'command':
      term.write(`\r\n${stamp}\x1b[36m$ ${event.line}\x1b[0m\r\n`)
      break
    case 'stderr':
      term.write(`${stamp}\x1b[31m${event.line}\x1b[0m\r\n`)
      break
    case 'exit':
      term.write(`${stamp}\x1b[90m—— 退出码 ${event.line} ——\x1b[0m\r\n`)
      break
    default:
      term.write(`${stamp}${event.line}\r\n`)
  }

  lineCount.value += 1
}

/**
 * `[HH:MM:SS]` —— 终端里的「等宽指令」必须够用，且本机时区（与服务时间可能不同，
 * 但中继进程也不起服务进程关系）。
 */
function formatTimestamp(date: Date): string {
  const pad = (value: number) => value.toString().padStart(2, '0')
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
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

async function copy() {
  const term = terminal.value
  if (!term) return
  // xterm 6 的 IBuffer 只有 getLine（没有 getLines），所以自己走一遍
  const buffer = term.buffer.active
  const lines: string[] = []
  for (let y = 0; y < buffer.length; y += 1) {
    lines.push(buffer.getLine(y)?.translateToString(true) ?? '')
  }

  // ⚠️ 走 Tauri 的剪贴板插件（capabilities 里唯一为它开的权限），而不是 webview 的
  // `navigator.clipboard`：后者在这台机器上可能被拒，而从前那句 `void …` 把 rejection
  // 也吞了 —— 用户点「复制」毫无反应、也没有任何提示（H-6）。
  try {
    await copyText(lines.join('\n').trimEnd())
    copyState.value = 'copied'
  } catch (error) {
    copyState.value = 'failed'
    console.warn('[peon-hall] 复制命令行输出失败', error)
  }
  timers.push(
    window.setTimeout(() => {
      copyState.value = 'idle'
    }, COPY_STATE_MS),
  )
}

/** 复制按钮的文案：平时「复制」，点完给 1.5 秒的「已复制 / 复制失败」。 */
const copyLabel = computed(() => {
  if (copyState.value === 'copied') return drawer.copied
  if (copyState.value === 'failed') return drawer.copyFailed
  return drawer.copy
})

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
    // 终端自身透明（毛玻璃由外层 .drawer 负责）；
    // 滚动条颜色走主题 —— xterm 6 的滚动条是它自己画的，CSS 伪元素对它无效
    theme: {
      background: 'rgba(0, 0, 0, 0)',
      foreground: '#e5e7eb',
      cursor: '#0b0f14',
      scrollbarSliderBackground: 'rgba(255, 255, 255, 0.16)',
      scrollbarSliderHoverBackground: 'rgba(255, 255, 255, 0.3)',
      scrollbarSliderActiveBackground: 'rgba(255, 255, 255, 0.45)',
    },
  })
  term.loadAddon(fitAddon)
  searchAddon = new SearchAddon()
  term.loadAddon(searchAddon)
  term.open(host.value!)
  terminal.value = term
  await fit()

  // 命令事件由 Rust 侧发（每次中继调用都会发一串）
  stopListening = await onCommandEvent(write)

  // Ctrl/Cmd+F 唤起搜索框（xterm 自带的快捷键会先抢；但它现在没注册——手动接管）
  term.attachCustomKeyEventHandler((event) => {
    if ((event.ctrlKey || event.metaKey) && event.key === 'f' && event.type === 'keydown') {
      event.preventDefault()
      searchVisible.value = true
      return false
    }
    return true
  })

  window.addEventListener('resize', fit)
})

onBeforeUnmount(() => {
  window.removeEventListener('resize', fit)
  stopListening?.()
  searchAddon?.dispose()
  searchAddon = null
  terminal.value?.dispose()
  timers.forEach((timer) => window.clearTimeout(timer))
})

/** 输入框变化时实时搜索；找不到就高亮 'status="error"'。 */
function onSearchInput(event: Event): void {
  const target = event.target as HTMLInputElement | null
  const value = target?.value ?? ''
  searchQuery.value = value
  if (!searchAddon || !value) {
    searchNotFound.value = false
    searchAddon?.clearDecorations()
    return
  }
  const found = searchAddon.findNext(value)
  searchNotFound.value = !found
}

/** Enter 跳下一个；Shift+Enter 跳上一个。 */
function onSearchKey(event: KeyboardEvent): void {
  if (!searchAddon || !searchQuery.value) return
  if (event.key === 'Enter') {
    event.preventDefault()
    const found = event.shiftKey
      ? searchAddon.findPrevious(searchQuery.value)
      : searchAddon.findNext(searchQuery.value)
    searchNotFound.value = !found
  }
  else if (event.key === 'Escape') {
    searchVisible.value = false
  }
}

/** 关闭搜索框时清掉高亮。 */
function closeSearch(): void {
  searchVisible.value = false
  searchQuery.value = ''
  searchNotFound.value = false
  searchAddon?.clearDecorations()
}

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
    <!-- 面板：高度 0 ↔ 90%，视觉上就是从 footer 由下往上抽出来 -->
    <section class="drawer-panel" :class="{ 'drawer-panel-open': open }" :aria-hidden="!open">
      <header class="drawer-header">
        <span class="drawer-title">{{ drawer.title }}</span>
        <ASpace :size="4">
          <AButton size="small" type="text" @click="clear">{{ drawer.clear }}</AButton>
          <AButton size="small" type="text" @click="copy">{{ copyLabel }}</AButton>
          <AButton size="small" type="text" @click="emit('toggle', false)">
            {{ drawer.close }}
          </AButton>
        </ASpace>
      </header>

      <!--
        搜索框：Ctrl/Cmd+F 唤起，Enter 跳下一个，Shift+Enter 跳上一个，Esc 关闭。
        为什么不用 xterm 自带的快捷键：它占的是「终端内部」语义（find within buffer），
        但这里要让用户能看见在搜什么 —— 输入框可见才是关键。
      -->
      <div v-if="searchVisible" class="drawer-search">
        <AInput
          v-model:value="searchQuery"
          size="small"
          :placeholder="drawer.searchPlaceholder"
          :status="searchNotFound ? 'error' : ''"
          autofocus
          allow-clear
          @input="onSearchInput"
          @keydown="onSearchKey"
          @keydown.escape="closeSearch"
        />
        <AButton size="small" type="text" @click="closeSearch">{{ drawer.close }}</AButton>
      </div>

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
  /* ⚠️ **不是** fixed：它是内容区的兄弟节点，占自己的一份高度 ——
     fixed 会把底部内容压在下面（早期版本靠 padding 补偿，窗口一窄就不够）。 */
  z-index: 30;
  display: flex;
  flex: none;
  flex-direction: column;
  /* 整块面板是毛玻璃 */
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
  /* 展开后占视口的大部分，但内容区仍有 30vh 的下限（见 App.vue 的 .shell__body） ——
     这样「看输出」和「看状态」能同时在屏幕上，谁也不会被谁盖住。 */
  height: 62vh;
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

.drawer-search {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 6px 12px;
  border-bottom: 1px solid #1f2937;
}

.drawer-search :deep(.ant-input-affix-wrapper) {
  background: rgb(17 24 39 / 55%);
  border-color: rgb(255 255 255 / 8%);
}
</style>

<!--
  ⚠️ 这里刻意**不加 scoped**：`.xterm-scrollable-element` 是 xterm 运行时插进 DOM 的，
  不会带上 SFC 的 data-v 属性，scoped 样式匹配不到它。
-->
<style>
/* 终端滚动条：xterm 6 自绘（VS Code 那套），宽高都是 JS 写的**行内样式** —— 只能 !important。
   颜色不在这里管，走上面的主题选项（xterm 提供的唯一滚动条 API）。

   ⚠️ 宽度要覆盖**两处**：外层 `.scrollbar` 与里面的 `.slider` 各自带一个行内 width
   （实测 DOM：两个都是 `width: 14px`）。只改外层的话，滑块还是 14px，看着「没变」。 */
.drawer .xterm-scrollable-element > .scrollbar.vertical {
  width: 10px !important;
}

.drawer .xterm-scrollable-element > .scrollbar.vertical > .slider {
  left: 1px !important;
  width: 8px !important;
  border-radius: 9999px;
}

/* 横向同理（终端很少横向滚动，但保持一致） */
.drawer .xterm-scrollable-element > .scrollbar.horizontal {
  height: 10px !important;
}

.drawer .xterm-scrollable-element > .scrollbar.horizontal > .slider {
  top: 1px !important;
  height: 8px !important;
  border-radius: 9999px;
}
</style>
