<script setup lang="ts">
// 五个操作。可用性矩阵在 useRelayState 里算好，这里只渲染 ——
// 禁用的按钮一定带 title 说明原因（§ 7：不能只灰掉）。

import type { ActionItem } from '../composables/useRelayState'

const props = defineProps<{ items: ActionItem[] }>()

const emit = defineEmits<{ run: [name: ActionItem['name']] }>()

function variantClass(item: ActionItem): string {
  if (item.disabled) return 'btn-secondary'
  if (item.variant === 'primary') return 'btn-primary'
  if (item.variant === 'danger') return 'btn-danger'
  return 'btn-secondary'
}
</script>

<template>
  <!--
    title 放在外层 span 上：原生 disabled 的按钮在部分浏览器上不弹 tooltip
    （禁用元素不派发鼠标事件），所以给按钮加 pointer-events-none，让外层接管悬停。
  -->
  <div class="flex flex-wrap gap-2">
    <span
      v-for="item in props.items"
      :key="item.name"
      class="inline-flex"
      :class="{ 'cursor-not-allowed': item.disabled }"
      :title="item.disabled ? item.reason : undefined"
    >
      <button
        type="button"
        :class="[variantClass(item), item.disabled ? 'pointer-events-none' : '']"
        :disabled="item.disabled"
        :title="item.disabled ? item.reason : undefined"
        @click="emit('run', item.name)"
      >
        <span v-if="item.busy" class="spinner" />
        {{ item.label }}
      </button>
    </span>
  </div>
</template>
