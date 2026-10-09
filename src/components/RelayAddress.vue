<script setup lang="ts">
// 扩展地址 + 复制按钮（验收 #7：复制的内容必须与发现文件里的 url 完全一致）。

import { info as infoText, misc } from '../locales/zh-CN'
import type { UrlSource } from '../types'

const props = defineProps<{
  url: string
  /** 地址是从哪来的（控制面 / 配置文件 / 默认值）。 */
  source: UrlSource
  copied: boolean
}>()

const emit = defineEmits<{ copy: [] }>()
</script>

<template>
  <div class="info-row flex-wrap">
    <span class="info-row__label">{{ infoText.relayAddress }}</span>
    <span class="info-row__value mono selectable">
      {{ props.url === '' ? infoText.unknown : props.url }}
    </span>
    <span class="source">{{ infoText.urlSource[props.source] }}</span>
    <button
      type="button"
      class="btn-secondary"
      :disabled="props.url === ''"
      :title="props.url === '' ? misc.noAddress : undefined"
      @click="emit('copy')"
    >
      {{ props.copied ? misc.copied : misc.copyAddress }}
    </button>
  </div>
</template>

<style scoped>
.source {
  flex: none;
  color: var(--muted);
  font-size: 12px;
}
</style>
