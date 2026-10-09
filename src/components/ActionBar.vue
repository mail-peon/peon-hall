<script setup lang="ts">
// 五个操作。可用性矩阵在 useRelayState 里算好，这里只渲染 ——
// 禁用的按钮一定带 tooltip 说明原因（§ 7：不能只灰掉）。

import { Button as AButton, Space as ASpace, Tooltip as ATooltip } from 'ant-design-vue'

import type { ActionItem } from '../composables/useRelayState'

const props = defineProps<{ items: ActionItem[] }>()

const emit = defineEmits<{ run: [name: ActionItem['name']] }>()

/** 「安装/启动」是主操作，其余用默认按钮；卸载走 danger。 */
function buttonType(item: ActionItem): 'primary' | 'default' {
  return item.variant === 'primary' ? 'primary' : 'default'
}
</script>

<template>
  <ASpace class="action-bar" :size="8" wrap>
    <ATooltip
      v-for="item in props.items"
      :key="item.name"
      :title="item.disabled ? item.reason : ''"
    >
      <!-- antd 的 Tooltip 需要能接收鼠标事件的子元素：禁用按钮外面包一层 -->
      <span>
        <AButton
          :type="buttonType(item)"
          :danger="item.variant === 'danger'"
          :loading="item.busy"
          :disabled="item.disabled"
          @click="emit('run', item.name)"
        >
          {{ item.label }}
        </AButton>
      </span>
    </ATooltip>
  </ASpace>
</template>

<style scoped>
.action-bar {
  margin-top: 12px;
}
</style>
