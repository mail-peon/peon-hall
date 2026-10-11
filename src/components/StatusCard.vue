<script setup lang="ts">
// 状态卡片：一个状态徽标 + 标题 + 副标题。
// 按钮在 ActionBar 里 —— 卡片只回答「现在是什么状态」（ai-docs/01-ui-and-states.md § 2）。

import { Badge as ABadge, Card as ACard } from 'ant-design-vue'
import { computed } from 'vue'

const props = defineProps<{
  title: string
  subtitle: string
  /** 状态色名（running / stopped / foreground / unknown / none）。 */
  tone: string
}>()

/** 把内部的状态色名映射成 antd 的 Badge 状态词（antd 只认这几个）。 */
const badgeStatus = computed(() => {
  switch (props.tone) {
    case 'running':
      return 'success'
    case 'foreground':
      return 'processing'
    // 「读不到服务状态」是**需要看一眼**的状态（黄），不是「未安装/安装不完整」的红
    case 'unknown':
      return 'warning'
    case 'none':
      return 'error'
    default:
      return 'default'
  }
})
</script>

<template>
  <ACard size="small">
    <ABadge :status="badgeStatus" :text="props.title" class="status-badge" />
    <p class="subtitle">{{ props.subtitle }}</p>
  </ACard>
</template>

<style scoped>
.status-badge :deep(.ant-badge-status-text) {
  color: inherit;
  font-size: 16px;
  font-weight: 600;
}

.subtitle {
  margin: 4px 0 0;
  color: var(--muted);
  font-size: 12.5px;
  line-height: 1.5;
}
</style>
