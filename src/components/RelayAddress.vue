<script setup lang="ts">
// 扩展地址 + 复制按钮（验收 #7：复制的内容必须与发现文件里的 url 完全一致）。
// 标签由外层（App 的 Descriptions）给，这里只渲染值 + 来源 + 按钮。

import { Button as AButton, Tag as ATag, Tooltip as ATooltip, Typography as ATypography } from 'ant-design-vue'

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
  <div class="relay-address flex flex-wrap items-center gap-2">
    <ATypography.Text code class="selectable">
      {{ props.url === '' ? infoText.unknown : props.url }}
    </ATypography.Text>

    <ATag :bordered="false">{{ infoText.urlSource[props.source] }}</ATag>

    <ATooltip :title="props.url === '' ? misc.noAddress : ''">
      <span>
        <AButton
          size="small"
          :disabled="props.url === ''"
          @click="emit('copy')"
        >
          {{ props.copied ? misc.copied : misc.copyAddress }}
        </AButton>
      </span>
    </ATooltip>
  </div>
</template>
