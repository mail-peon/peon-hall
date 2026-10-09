<script setup lang="ts">
// 开机自启开关。只在已注册时可用；安装级别是系统服务时打个标记
// （那个级别下改自启也要管理员权限）。

import { computed } from 'vue'

import { info as infoText } from '../locales/zh-CN'
import type { Autostart, ServiceLevel } from '../types'

const props = defineProps<{
  /** 当前自启方式（undefined = 未安装或读不到）。 */
  value: Autostart | undefined
  level: ServiceLevel | undefined
  disabled: boolean
  /** 禁用原因（显示成 title）。 */
  reason: string
  busy: boolean
}>()

const emit = defineEmits<{ change: [on: boolean] }>()

const on = computed(() => props.value === 'logon' || props.value === 'boot')

function onChange(event: Event) {
  emit('change', (event.target as HTMLInputElement).checked)
}
</script>

<template>
  <label
    class="switch"
    :class="{ 'switch--disabled': props.disabled }"
    :title="props.disabled ? props.reason : undefined"
  >
    <input
      type="checkbox"
      role="switch"
      :checked="on"
      :disabled="props.disabled"
      @change="onChange"
    >
    <span class="switch__track" :class="{ 'switch__track--on': on }">
      <span class="switch__thumb" />
    </span>
    <span class="switch__label">{{ infoText.autostartValue(props.value) }}</span>
    <span v-if="props.busy" class="spinner" />
    <span v-if="props.level === 'system'" class="badge">{{ infoText.levelValue('system') }}</span>
  </label>
</template>

<style scoped>
.switch {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}

.switch--disabled {
  cursor: not-allowed;
}

.switch--disabled .switch__label {
  color: var(--muted);
}

/* 原生 checkbox 视觉上藏起来，但保留焦点与键盘操作 */
.switch input {
  position: absolute;
  width: 1px;
  height: 1px;
  opacity: 0;
}

.switch__track {
  position: relative;
  flex: none;
  width: 32px;
  height: 18px;
  border-radius: 9999px;
  background: var(--border);
  transition: background 0.15s;
}

.switch__track--on {
  background: var(--state-running);
}

.switch__thumb {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 14px;
  height: 14px;
  border-radius: 9999px;
  background: var(--surface);
  transition: transform 0.15s;
}

.switch__track--on .switch__thumb {
  transform: translateX(14px);
}

.switch__label {
  font-size: 13px;
}

.badge {
  padding: 1px 6px;
  border-radius: 9999px;
  background: var(--surface-2);
  color: var(--muted);
  font-size: 11px;
}

.switch input:disabled + .switch__track {
  opacity: 0.5;
}

.switch input:focus-visible + .switch__track {
  outline: 2px solid var(--state-running);
  outline-offset: 2px;
}
</style>
