import type { Ref } from 'vue'
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { Terminal } from '@xterm/xterm'
import { onCommandEvent, copyText } from '../ipc'
import type { CommandEvent } from '../types'
import { drawer } from '../locales/zh-CN'

/** Owns the terminal and its asynchronous event subscription for one mounted drawer. */
export function useCommandTerminal(host: Ref<HTMLDivElement | null>, isOpen: () => boolean) {
  const terminal = shallowRef<Terminal | null>(null)
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
    if (!isOpen()) return
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

  let disposed = false
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
    if (disposed) return
    const stop = await onCommandEvent(write)
    if (disposed) {
      stop()
      return
    }
    stopListening = stop

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
    disposed = true
    window.removeEventListener('resize', fit)
    stopListening?.()
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
    () => isOpen(),
    async (open) => {
      await fit()
      if (open) {
        // 展开动画（220ms）结束后再量一次，否则列数按动画中途的高度算
        timers.push(window.setTimeout(() => void fit(), 240))
      }
    },
  )

  return {
    lineCount, searchVisible, searchQuery, searchNotFound, copyLabel,
    clear, copy, onSearchInput, onSearchKey, closeSearch,
  }
}
