<script setup lang="ts">
// 单页：提示条 + 状态卡片 + 五个操作 + 信息行 + 高级（系统服务）+ 诊断 + 命令抽屉。
// 所有状态与交互逻辑都在 useRelayState 里，这里只做布局。
//
// 组件用 ant-design-vue（显式按需导入，保留 tree-shaking）。
//
// 布局：**整窗一列 flex，命令行是内容区的兄弟而不是浮层** ——
// 内容区自己滚动（`flex: 1` + `min-height: 0`），命令行占底部自己的一份高度。
// 早期版本用 `position: fixed` 把命令行压在内容上，结果底部总有一段被挡住
// （只能靠 padding 补偿，窗口一窄就不够）。同层布局从根上没这个问题。

import { Alert as AAlert, Button as AButton, Card as ACard, Collapse as ACollapse, CollapsePanel as ACollapsePanel, ConfigProvider as AConfigProvider, Descriptions as ADescriptions, DescriptionsItem as ADescriptionsItem, Tooltip as ATooltip } from 'ant-design-vue'
import zhCN from 'ant-design-vue/es/locale/zh_CN'
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

// 命令抽屉：默认收起，避免挤占内容区
const drawerOpen = ref(false)
</script>

<template>
  <AConfigProvider
    :locale="zhCN"
    :theme="{ token: { borderRadius: 8, colorPrimary: '#16a34a' } }"
  >
    <div class="shell">
      <!-- 内容区：自己滚动，命令行展开时它跟着变矮（而不是被盖住） -->
      <div class="shell__body">
        <div class="shell__scroll">
          <!-- 提示条：黄 = 需要知道（提权取消、超时），红 = 出错了（§ 3.1） -->
          <AAlert
            v-if="visibleNotice"
            class="mb-3"
            show-icon
            :type="visibleNotice.tone === 'warn' ? 'warning' : 'error'"
            :message="visibleNotice.text"
          >
            <template v-if="visibleNotice.action" #action>
              <AButton size="small" @click="runNoticeAction">
                {{ visibleNotice.action.label }}
              </AButton>
            </template>
          </AAlert>

          <StatusCard :title="phaseTitle" :subtitle="phaseSubtitle" :tone="phaseTone" />

          <ActionBar :items="actions" @run="runAction" />

          <ACard class="info-card" size="small">
            <ADescriptions :column="1" size="small">
              <ADescriptionsItem :label="infoText.autostart">
                <AutostartSwitch
                  :value="autostartValue"
                  :level="level"
                  :disabled="autostartDisabled"
                  :reason="autostartReason"
                  :busy="autostartBusy"
                  @change="toggleAutostart"
                />
              </ADescriptionsItem>

              <ADescriptionsItem :label="infoText.level">
                <span class="info-value">{{ levelValue }}</span>
              </ADescriptionsItem>

              <ADescriptionsItem :label="infoText.relayAddress">
                <RelayAddress
                  :url="relayUrl"
                  :source="urlSource"
                  :copied="addressCopied"
                  @copy="copyAddress"
                />
              </ADescriptionsItem>

              <ADescriptionsItem :label="infoText.version">
                <span class="info-value selectable">{{ versionText }}</span>
              </ADescriptionsItem>
            </ADescriptions>
          </ACard>

          <!-- 高级：默认的用户级安装不需要管理员权限，系统服务是例外 -->
          <ACollapse class="mt-3" :bordered="false">
            <ACollapsePanel key="advanced" :header="advanced.title">
              <p class="hint">{{ advanced.hint }}</p>
              <ATooltip :title="systemInstallDisabled ? systemInstallReason : ''">
                <span>
                  <AButton
                    :loading="systemInstallBusy"
                    :disabled="systemInstallDisabled"
                    @click="installAsSystemService"
                  >
                    {{ systemInstallBusy ? advanced.installSystemBusy : advanced.installSystem }}
                  </AButton>
                </span>
              </ATooltip>
            </ACollapsePanel>
          </ACollapse>

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
        </div>

        <!-- 毛玻璃遮罩只盖内容区（命令行不占它的位置），点它收起 -->
        <div
          v-if="drawerOpen"
          class="shell__scrim"
          data-testid="command-drawer-scrim"
          @click="drawerOpen = false"
        />
      </div>

      <CommandDrawer :open="drawerOpen" @toggle="drawerOpen = $event" />
    </div>
  </AConfigProvider>
</template>

<style scoped>
/* 整窗一列：内容区弹性 + 命令行按自己的高度占位 */
.shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
}

/* 内容区外壳：相对定位，给毛玻璃遮罩当地标；给个下限，免得命令行一展开内容全没了 */
.shell__body {
  position: relative;
  display: flex;
  flex: 1 1 auto;
  min-height: 30vh;
}

/* 真正滚动的那一层 */
.shell__scroll {
  flex: 1 1 auto;
  min-width: 0;
  padding: 12px;
  overflow-y: auto;
  scrollbar-gutter: stable;
}

.shell__scrim {
  position: absolute;
  inset: 0;
  background: rgb(3 7 12 / 35%);
  backdrop-filter: blur(3px) saturate(120%);
  cursor: pointer;
}

.info-card {
  margin-top: 12px;
}

.info-value {
  font-size: 13px;
}

.hint {
  margin: 0 0 10px;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.5;
}
</style>
