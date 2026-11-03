// 页面状态的**唯一来源**：四个状态、五个操作的可用性矩阵、轮询节奏、操作编排。
//
// 组件只渲染（见 ai-docs/00-overview.md § 5 的职责边界）；「点哪个按钮会发生什么」
// 全在这里，组件小到读一遍就知道它渲染什么。

import { computed, ref } from 'vue'

import * as ipc from '../ipc'
import {
  actionText,
  disabledReason,
  disabledReasonByPhase,
  info as infoText,
  misc,
  notice as noticeText,
  phaseText,
} from '../locales/zh-CN'
import {
  availabilityFor,
  phaseOf,
  variantOf as variantOfPure,
} from './relay-pure'
import type {
  ActionItem,
  CommandError,
  DiscoveryState,
  Heartbeat,
  Notice,
  Phase,
  ProcessStatus,
  ServiceActionName,
  ServiceStatus,
  Snapshot,
  UrlSource,
} from '../types'
import { doctorAction, noticeFromError } from './relay-feedback'
import { useRelayPolling } from './useRelayPolling'
import { useRelayActions } from './useRelayActions'
import { useRelayTools } from './useRelayTools'

/** 五个操作的固定顺序（当前状态的主 / 次按钮会被提到前面）。 */
const ACTION_ORDER: readonly ServiceActionName[] = [
  'install',
  'uninstall',
  'start',
  'stop',
  'restart',
]

export function useRelayState() {
  // ---- 快照里的状态（snapshot / heartbeat 两条路都会更新） ----
  const phase = ref<Phase>('notInstalled')
  const service = ref<ServiceStatus | null>(null)
  /** 读服务状态失败的原始原因（H-2：`null` = 这一次读到了）。 */
  const serviceError = ref<string | null>(null)
  const processStatus = ref<ProcessStatus | null>(null)
  const controlError = ref<string | null>(null)
  const discovery = ref<DiscoveryState>('missing')
  const discoveryDetail = ref('')
  const relayUrl = ref('')
  const urlSource = ref<UrlSource>('default')
  const coreBound = ref('')
  /** 桌面端自己的版本（`Snapshot.appVersion`；中继没在跑时版本行显示它，H-4）。 */
  const desktopVersion = ref('')
  const sidecarPresent = ref(true)

  // ---- 界面自身的状态 ----
  const loading = ref(true)
  const notice = ref<Notice | null>(null)

  /**
   * 状态组合的判据：**只**看「服务管理器说注册了吗」+「控制面说活着吗」。
   * 与 Rust 侧 `Snapshot::phase_of` 同一套（不能靠「端口连不上」推断：前台运行时端口是通的）。
   *
   * H-2：`serviceError` 有值 = 这一轮**没读到**服务状态 → `serviceUnknown`。
   * 那是「不知道」，不能猜成 `notInstalled`（否则界面会劝用户点【安装服务】）。
   */
  function derivePhase(current: ServiceStatus | null, process: ProcessStatus | null): Phase {
    return phaseOf(sidecarPresent.value, serviceError.value, current, process)
  }

  // ---- 应用状态 ----

  function applySnapshot(next: Snapshot) {
    phase.value = next.phase
    service.value = next.service
    serviceError.value = next.serviceError
    processStatus.value = next.process
    controlError.value = next.controlError
    discovery.value = next.discovery
    discoveryDetail.value = next.discoveryDetail
    relayUrl.value = next.relayUrl
    urlSource.value = next.urlSource
    coreBound.value = next.coreBound
    // 中继没在跑时版本行就靠它（H-4）：Rust 一直在传，从前这里没存
    desktopVersion.value = next.appVersion
    sidecarPresent.value = next.sidecarPresent
  }

  function applyHeartbeat(beat: Heartbeat) {
    processStatus.value = beat.process
    controlError.value = beat.controlError
    discovery.value = beat.discovery
    discoveryDetail.value = beat.discoveryDetail
    relayUrl.value = beat.relayUrl
    urlSource.value = beat.urlSource
    phase.value = derivePhase(service.value, beat.process)
  }

  function applyService(next: ServiceStatus) {
    // 这一轮读到了 → 把上一次的失败原因清掉（H-2：读得到就不再是「读不到」）
    service.value = next
    serviceError.value = null
    phase.value = derivePhase(next, processStatus.value)
  }

  function applyDetached(error: CommandError) {
    // 心跳失败不是「错误弹窗」：卡片自己会变成「已安装，未运行」，这里只是把进程状态清掉
    processStatus.value = null
    controlError.value = error.message
    phase.value = derivePhase(service.value, null)
  }

  // ---- 提示条 ----

  /**
   * 状态本身带来的提示（不是某次操作的结果）。
   *
   * - H-2：读不到服务状态 —— 原始原因在卡片副标题上，这里只给一个进诊断的入口；
   * - § 5：服务管理器说「在跑」但控制面连不上 —— 多半正在重启 / 更新，说清楚别让用户以为坏了。
   */
  const statusNotice = computed<Notice | null>(() => {
    if (serviceError.value !== null) {
      return { tone: 'warn', text: noticeText.serviceUnknown, action: doctorAction() }
    }
    if (service.value?.running !== true || processStatus.value !== null) return null
    return {
      tone: 'warn',
      text: noticeText.controlUnreachable(controlError.value ?? discoveryDetail.value),
    }
  })

  /** 操作结果优先于状态提示。 */
  const visibleNotice = computed<Notice | null>(() => notice.value ?? statusNotice.value)

  const { refresh } = useRelayPolling({
    applySnapshot, applyHeartbeat, applyService, applyDetached,
    onRefreshError(error) {
      if (notice.value === null) notice.value = noticeFromError(error)
    },
    onLoaded() { loading.value = false },
  })
  const { busy, pendingMode, runGuarded, runAction, installAsSystemService, toggleAutostart } = useRelayActions({
    notice, phase, serviceError, refresh: () => refresh(true),
  })
  const { addressCopied, copyAddress, doctorOpen, doctorReport, doctorLoading, doctorError,
    doctorCopied, setDoctorOpen, openDoctor, loadDoctor, copyDoctor, openConfigDir, openLogsDir,
  } = useRelayTools(relayUrl, notice)

  function runNoticeAction() {
    const action = visibleNotice.value?.action
    notice.value = null
    if (action?.kind === 'doctor') openDoctor()
    else if (action?.kind === 'uninstall') {
      // H-7：撤销安装走的就是普通卸载（提权按当前安装级别），结果照常显示在提示条上
      void runGuarded('uninstall', () => ipc.serviceAction('uninstall'))
    } else void refresh()
  }

  // ---- 可用性矩阵（§ 3「何时可用」） ----

  /**
   * 所有操作都禁用的两种状态：
   * - `incomplete`：安装包不完整（§ 5）；
   * - `serviceUnknown`：读不到服务状态（H-2）—— 不知道装没装，就不该让用户点【安装服务】。
   */
  const allBlocked = computed(
    () => phase.value === 'incomplete' || phase.value === 'serviceUnknown',
  )

  /** 全禁用时的统一原因（两种情况的原因不同，别把「读不到状态」说成「缺中继程序」）。 */
  const blockedReason = computed(() =>
    phase.value === 'serviceUnknown' ? misc.serviceUnknownDisabled : noticeText.sidecarMissing,
  )

  const availability = computed<Record<ServiceActionName, boolean>>(() => {
    const running = processStatus.value !== null
    return availabilityFor(service.value?.installed === true, running)
  })

  function canRun(name: ServiceActionName): boolean {
    if (allBlocked.value || loading.value || busy.value !== null) return false
    return availability.value[name]
  }

  /** 禁用原因：先排掉全局原因，再看这个操作在当前状态下的原因。 */
  function actionReason(name: ServiceActionName): string {
    if (busy.value !== null) return misc.busy
    if (loading.value) return misc.loading
    if (allBlocked.value) return blockedReason.value
    return disabledReasonByPhase[phase.value][name] ?? disabledReason[name] ?? misc.actionNotAvailable
  }

  /** 主按钮用主色；卸载是破坏性的，永远用红色提醒。 */
  function variantOf(name: ServiceActionName, primary: ServiceActionName | null): ActionItem['variant'] {
    return variantOfPure(name, primary)
  }

  const actions = computed<ActionItem[]>(() => {
    const { primary, secondary } = phaseText[phase.value]
    const rest = ACTION_ORDER.filter((name) => name !== primary && name !== secondary)
    return [primary, secondary, ...rest]
      .filter((name): name is ServiceActionName => name !== null)
      .map((name) => ({
        name,
        label: busy.value === name ? actionText[name].busy : actionText[name].label,
        variant: variantOf(name, primary),
        disabled: !canRun(name),
        reason: actionReason(name),
        busy: busy.value === name,
      }))
  })

  const systemInstallBusy = computed(() => busy.value === 'install' && pendingMode.value === 'system')
  const systemInstallDisabled = computed(() => !canRun('install'))
  const systemInstallReason = computed(() => actionReason('install'))

  // ---- 信息行 ----

  const level = computed(() => service.value?.level)
  const levelValue = computed(() => infoText.levelValue(service.value?.level))
  const autostartValue = computed(() => service.value?.autostart)
  const autostartBusy = computed(() => busy.value === 'autostart')
  const autostartDisabled = computed(
    () =>
      allBlocked.value || loading.value || busy.value !== null || service.value?.installed !== true,
  )
  const autostartReason = computed(() => {
    if (busy.value !== null) return misc.busy
    if (loading.value) return misc.loading
    if (allBlocked.value) return blockedReason.value
    return misc.autostartDisabled
  })

  /** 中继没在跑就不显示它的版本号（验收 #12）；桌面端自己的版本号永远显示。 */
  const versionText = computed(() =>
    infoText.versionValue(
      processStatus.value?.version,
      desktopVersion.value,
      coreBound.value,
    ),
  )

  return {
    phaseTitle: computed(() => phaseText[phase.value].title),
    phaseSubtitle: computed(() =>
      phaseText[phase.value].subtitle({
        process: processStatus.value,
        service: service.value ?? {},
        serviceError: serviceError.value,
      }),
    ),
    phaseTone: computed(() => phaseText[phase.value].tone),

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
  }
}
