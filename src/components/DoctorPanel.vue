<script setup lang="ts">
// 诊断面板：把 doctor 的每一项渲染成 标记 + 标题 + 细节（§ 6）。
// 逐项照原样渲染 —— 凭据在 core 侧就已经被排除掉了，前端不做二次加工。

import { Alert as AAlert, Button as AButton, Collapse as ACollapse, CollapsePanel as ACollapsePanel, List as AList, ListItem as AListItem, Space as ASpace, Spin as ASpin, Tag as ATag, Tooltip as ATooltip } from 'ant-design-vue'
import { computed } from 'vue'

import { doctorText } from '../locales/zh-CN'
import type { CheckLevel, DoctorReport } from '../types'

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

/** antd 的 Collapse 用 activeKey 表达展开，只在真的变了才往外抛。 */
const activeKey = computed(() => (props.open ? ['doctor'] : []))

function onCollapseChange(key: string | number | (string | number)[]) {
  const open = Array.isArray(key) ? key.length > 0 : true
  if (open !== props.open) emit('toggle', open)
}

/** 检查结果的标记（antd 的 List 把 renderItem 的参数暴露成 any，这里收口一次断言）。 */
function mark(level: string): string {
  return doctorText.mark[level as CheckLevel] ?? ''
}

/** 检查结果的颜色：正常/警告/失败。 */
function tagColor(level: CheckLevel): string {
  switch (level) {
    case 'ok':
      return 'green'
    case 'warn':
      return 'orange'
    default:
      return 'red'
  }
}
</script>

<template>
  <ACollapse
    class="mt-3"
    :bordered="false"
    :active-key="activeKey"
    @change="onCollapseChange"
  >
    <ACollapsePanel key="doctor" :header="doctorText.title">
      <div v-if="props.loading" class="flex items-center gap-2">
        <ASpin size="small" />
        <span class="hint">{{ doctorText.loading }}</span>
      </div>

      <AAlert
        v-else-if="props.error !== null"
        type="error"
        show-icon
        :message="doctorText.failed"
        :description="props.error"
      />

      <AList
        v-else-if="props.report !== null"
        size="small"
        :data-source="props.report.checks"
      >
        <template #renderItem="{ item }">
          <AListItem>
            <AListItem.Meta>
              <template #title>
                <span class="flex items-center gap-2">
                  <ATag :color="tagColor(item.level)" :bordered="false">
                    {{ mark(item.level) }}
                  </ATag>
                  <span>{{ item.title }}</span>
                </span>
              </template>
              <template #description>
                <div v-if="item.detail !== ''" class="check-detail selectable">
                  {{ item.detail }}
                </div>
                <!-- core 的 action 是给人看的建议（自由文本），照原样显示 -->
                <div v-if="item.action" class="check-detail selectable">
                  {{ doctorText.suggestion }}{{ item.action }}
                </div>
              </template>
            </AListItem.Meta>

            <template v-if="item.action" #actions>
              <AButton size="small" @click="emit('open-config')">
                {{ doctorText.openConfig }}
              </AButton>
            </template>
          </AListItem>
        </template>
      </AList>

      <ASpace class="mt-3" :size="8" wrap>
        <ATooltip :title="props.report === null ? doctorText.copyUnavailable : ''">
          <span>
            <AButton
              type="primary"
              :disabled="props.report === null"
              @click="emit('copy')"
            >
              {{ props.copied ? doctorText.copied : doctorText.copyAll }}
            </AButton>
          </span>
        </ATooltip>

        <AButton @click="emit('open-config')">{{ doctorText.openConfig }}</AButton>
        <AButton @click="emit('open-logs')">{{ doctorText.openLogs }}</AButton>
        <AButton :loading="props.loading" @click="emit('refresh')">
          {{ doctorText.refresh }}
        </AButton>
      </ASpace>
    </ACollapsePanel>
  </ACollapse>
</template>

<style scoped>
.hint {
  color: var(--muted);
  font-size: 12px;
}

.check-detail {
  color: var(--muted);
  font-size: 12px;
  line-height: 1.6;
  word-break: break-all;
}
</style>
