<script setup lang="ts">
// 诊断面板：把 doctor 的每一项渲染成 前缀 + 标题 + 细节（§ 6）。
// 逐项照原样渲染 —— 凭据在 core 侧就已经被排除掉了，前端不做二次加工。

import { doctorText } from '../locales/zh-CN'
import type { DoctorReport } from '../types'

const props = defineProps<{
  open: boolean
  report: DoctorReport | null
  loading: boolean
  error: string | null
  copied: boolean
}>()

const emit = defineEmits<{
  toggle: [open: boolean]
  refresh: []
  copy: []
  'open-config': []
  'open-logs': []
}>()

/** 只在值真的变了才往外抛，避免 <details> 的 open 属性来回抖动。 */
function onToggle(event: Event) {
  const target = event.target as HTMLDetailsElement
  if (target.open !== props.open) emit('toggle', target.open)
}
</script>

<template>
  <details class="card py-3" :open="props.open" @toggle="onToggle">
    <summary>{{ doctorText.title }}</summary>

    <p v-if="props.loading" class="hint">{{ doctorText.loading }}</p>

    <template v-else-if="props.error !== null">
      <p class="hint">{{ doctorText.failed }}</p>
      <p class="check-item__detail selectable">{{ props.error }}</p>
    </template>

    <div v-else-if="props.report !== null" class="check-list mt-2">
      <div v-for="check in props.report.checks" :key="check.id" class="check-item">
        <span>{{ doctorText.mark[check.level] }}</span>
        <div class="min-w-0">
          <div>{{ check.title }}</div>
          <div v-if="check.detail !== ''" class="check-item__detail selectable">
            {{ check.detail }}
          </div>
          <!-- core 的 action 是给人看的建议（自由文本），照原样显示 -->
          <div v-if="check.action" class="check-item__detail selectable">
            {{ doctorText.suggestion }}{{ check.action }}
          </div>
        </div>
        <button
          v-if="check.action"
          type="button"
          class="btn-secondary"
          @click="emit('open-config')"
        >
          {{ doctorText.openConfig }}
        </button>
      </div>
    </div>

    <div class="mt-3 flex flex-wrap gap-2">
      <button
        type="button"
        class="btn-primary"
        :disabled="props.report === null"
        :title="props.report === null ? doctorText.copyUnavailable : undefined"
        @click="emit('copy')"
      >
        {{ props.copied ? doctorText.copied : doctorText.copyAll }}
      </button>
      <button type="button" class="btn-secondary" @click="emit('open-config')">
        {{ doctorText.openConfig }}
      </button>
      <button type="button" class="btn-secondary" @click="emit('open-logs')">
        {{ doctorText.openLogs }}
      </button>
      <button
        type="button"
        class="btn-secondary"
        :disabled="props.loading"
        :title="props.loading ? doctorText.loading : undefined"
        @click="emit('refresh')"
      >
        {{ doctorText.refresh }}
      </button>
    </div>
  </details>
</template>

<style scoped>
.hint {
  margin: 8px 0 0;
  color: var(--muted);
  font-size: 12px;
}
</style>
