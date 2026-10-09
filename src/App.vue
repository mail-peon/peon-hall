<script setup lang="ts">
// 单页：提示条 + 状态卡片 + 五个操作 + 信息行 + 高级（系统服务）+ 诊断（§ 2~§ 6）。
// 所有状态与交互逻辑都在 useRelayState 里，这里只做布局。

import { ref } from 'vue'

import ActionBar from './components/ActionBar.vue'
import AutostartSwitch from './components/AutostartSwitch.vue'
import CommandDrawer from './components/CommandDrawer.vue'
import DoctorPanel from './components/DoctorPanel.vue'
import RelayAddress from './components/RelayAddress.vue'
import StatusCard from './components/StatusCard.vue'
import { useRelayState } from './composables/useRelayState'
import { advanced, info as infoText } from './locales/zh-CN'

const {
  phaseTitle,
  phaseSubtitle,
  phaseTone,
  visibleNotice,
  runNoticeAction,
  actions,
  runAction,
  installAsSystemService,
  systemInstallBusy,
  systemInstallDisabled,
  systemInstallReason,
  level,
  levelValue,
  autostartValue,
  autostartBusy,
  autostartDisabled,
  autostartReason,
  toggleAutostart,
  versionText,
  relayUrl,
  urlSource,
  addressCopied,
  copyAddress,
  doctorOpen,
  doctorReport,
  doctorLoading,
  doctorError,
  doctorCopied,
  setDoctorOpen,
  loadDoctor,
  copyDoctor,
  openConfigDir,
  openLogsDir,
} = useRelayState()

// 命令抽屉：默认收起，避免挡住主操作
const drawerOpen = ref(false)
</script>

<template>
  <div class="app app-with-footer overflow-y-auto">
    <!-- 提示条：黄 = 需要知道（提权取消、超时），红 = 出错了（§ 3.1） -->
    <div
      v-if="visibleNotice"
      class="notice"
      :class="visibleNotice.tone === 'warn' ? 'notice--warn' : 'notice--error'"
    >
      <span class="flex-1 selectable">{{ visibleNotice.text }}</span>
      <button v-if="visibleNotice.action" type="button" class="btn-secondary" @click="runNoticeAction">
        {{ visibleNotice.action.label }}
      </button>
    </div>

    <StatusCard :title="phaseTitle" :subtitle="phaseSubtitle" :tone="phaseTone" />

    <ActionBar :items="actions" @run="runAction" />

    <section class="card flex flex-col py-3">
      <div class="info-row">
        <span class="info-row__label">{{ infoText.autostart }}</span>
        <AutostartSwitch
          :value="autostartValue"
          :level="level"
          :disabled="autostartDisabled"
          :reason="autostartReason"
          :busy="autostartBusy"
          @change="toggleAutostart"
        />
      </div>
      <div class="info-row">
        <span class="info-row__label">{{ infoText.level }}</span>
        <span class="info-row__value">{{ levelValue }}</span>
      </div>
      <RelayAddress
        :url="relayUrl"
        :source="urlSource"
        :copied="addressCopied"
        @copy="copyAddress"
      />
      <div class="info-row">
        <span class="info-row__label">{{ infoText.version }}</span>
        <span class="info-row__value selectable">{{ versionText }}</span>
      </div>
    </section>

    <!-- 高级：默认的用户级安装不需要管理员权限，系统服务是例外 -->
    <details class="card py-3">
      <summary>{{ advanced.title }}</summary>
      <p class="hint">{{ advanced.hint }}</p>
      <!-- 同 ActionBar：禁用的按钮弹不出 tooltip，原因放外层 -->
      <span
        class="inline-flex mt-2"
        :class="{ 'cursor-not-allowed': systemInstallDisabled }"
        :title="systemInstallDisabled ? systemInstallReason : undefined"
      >
        <button
          type="button"
          class="btn-secondary"
          :class="systemInstallDisabled ? 'pointer-events-none' : ''"
          :disabled="systemInstallDisabled"
          :title="systemInstallDisabled ? systemInstallReason : undefined"
          @click="installAsSystemService"
        >
          <span v-if="systemInstallBusy" class="spinner" />
          {{ systemInstallBusy ? advanced.installSystemBusy : advanced.installSystem }}
        </button>
      </span>
    </details>

    <DoctorPanel
      :open="doctorOpen"
      :report="doctorReport"
      :loading="doctorLoading"
      :error="doctorError"
      :copied="doctorCopied"
      @toggle="setDoctorOpen"
      @refresh="loadDoctor"
      @copy="copyDoctor"
      @open-config="openConfigDir"
      @open-logs="openLogsDir"
    />

    <CommandDrawer :open="drawerOpen" @toggle="drawerOpen = $event" />
  </div>
</template>

<style scoped>
/* 底部常驻命令 footer 的高度（固定定位，所以内容要自己让开） */
.app-with-footer {
  padding-bottom: 40px;
}

.hint {
  margin: 8px 0 0;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.5;
}
</style>
