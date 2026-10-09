<script setup lang="ts">
// 开机自启开关。只在已注册时可用；安装级别是系统服务时打个标记
// （那个级别下改自启也要管理员权限）。

import { Switch as ASwitch, Tag as ATag, Tooltip as ATooltip } from 'ant-design-vue'
import { computed } from 'vue'

import { info as infoText } from '../locales/zh-CN'
import type { Autostart, ServiceLevel } from '../types'

const props = defineProps<{
  /** 当前自启方式（undefined = 未安装或读不到）。 */
  value: Autostart | undefined
  level: ServiceLevel | undefined
  disabled: boolean
  /** 禁用原因（显示成 tooltip）。 */
  reason: string
  busy: boolean
}>()

const emit = defineEmits<{ change: [on: boolean] }>()

const on = computed(() => props.value === 'logon' || props.value === 'boot')

/** antd 的 Switch 会把值原样回调（boolean | string | number），这里统一收成 boolean。 */
function onChange(checked: boolean | string | number) {
  emit('change', checked === true)
}
</script>

<template>
  <div class="autostart flex items-center gap-2">
    <ATooltip :title="props.disabled ? props.reason : ''">
      <span>
        <ASwitch
          :checked="on"
          :disabled="props.disabled"
          :loading="props.busy"
          @change="onChange"
        />
      </span>
    </ATooltip>
    <span class="autostart__label">{{ infoText.autostartValue(props.value) }}</span>
    <ATag v-if="props.level === 'system'" color="blue">
      {{ infoText.levelValue('system') }}
    </ATag>
  </div>
</template>

<style scoped>
.autostart__label {
  font-size: 13px;
}
</style>
