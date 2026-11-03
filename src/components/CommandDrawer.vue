<script setup lang="ts">
import '@xterm/xterm/css/xterm.css'
import { Button as AButton, Input as AInput, Space as ASpace } from 'ant-design-vue'
import { ref } from 'vue'
import { drawer } from '../locales/zh-CN'
import { useCommandTerminal } from '../composables/useCommandTerminal'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ (event: 'toggle', open: boolean): void }>()
const host = ref<HTMLDivElement | null>(null)
const {
  lineCount, searchVisible, searchQuery, searchNotFound, copyLabel,
  clear, copy, onSearchInput, onSearchKey, closeSearch,
} = useCommandTerminal(host, () => props.open)
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
