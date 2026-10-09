// 页面状态的**唯一来源**：四个状态、五个操作的可用性矩阵、轮询节奏、操作编排。
//
// 组件只渲染（见 ai-docs/00-overview.md § 5 的职责边界）；「点哪个按钮会发生什么」
// 全在这里，组件小到读一遍就知道它渲染什么。

import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

import * as ipc from '../ipc'
import {
  actionText,
  disabledReason,
  disabledReasonByPhase,
  doctorText,
  info as infoText,
  misc,
  notice as noticeText,
  phaseText,
} from '../locales/zh-CN'
import type {
  ActionResult,
  CommandError,
  DiscoveryState,
  DoctorReport,
  Heartbeat,
  Phase,
  ProcessStatus,
  ServiceActionName,
  ServiceLevel,
  ServiceStatus,
  Snapshot,
  UrlSource,
} from '../types'

/** 「进行中」的目标：五个操作之一，或开机自启开关。 */
export type BusyTarget = ServiceActionName | 'autostart'

/** 提示条上的按钮：点它做什么。 */
export interface NoticeAction {
  label: string
  kind: 'doctor' | 'refresh'
}

/** 顶部提示条：黄 = 需要知道（**不是**错误），红 = 出错了。 */
export interface Notice {
  tone: 'warn' | 'error'
  text: string
  action?: NoticeAction
}

/** 操作栏的一项：可用性矩阵算出来的结果。 */
export interface ActionItem {
  name: ServiceActionName
  label: string
  variant: 'primary' | 'danger' | 'secondary'
  disabled: boolean
  /** 禁用原因（禁用时显示成 title —— § 7：不能只灰掉不说为什么）。 */
  reason: string
  busy: boolean
}

const POLL_MS = 2000
/** ping 连续失败 3 次后退避到 10 秒：控制面断开时别刷屏（§ 4）。 */
const POLL_BACKOFF_MS = 10_000
const FAILURES_BEFORE_BACKOFF = 3
/** 问服务管理器是**一次进程调用**（schtasks / launchctl / systemctl），每 5 次心跳问一次就够。 */
const SERVICE_STATUS_EVERY = 5
/** 提权 + 启动可能要十几秒；超过 30 秒给「刷新状态」，但**不**取消调用（§ 3.1）。 */
const ACTION_TIMEOUT_MS = 30_000
/** 「已复制」提示显示多久。 */
const COPIED_MS = 1500

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
  const processStatus = ref<ProcessStatus | null>(null)
  const controlError = ref<string | null>(null)
  const discovery = ref<DiscoveryState>('missing')
  const discoveryDetail = ref('')
  const relayUrl = ref('')
  const urlSource = ref<UrlSource>('default')
  const coreBound = ref('')
  const sidecarPresent = ref(true)

  // ---- 界面自身的状态 ----
  const loading = ref(true)
  const busy = ref<BusyTarget | null>(null)
  /** 只有点「安装为系统服务」时才有值：用来区分那条进行中文案。 */
  const pendingMode = ref<ServiceLevel | null>(null)
  const notice = ref<Notice | null>(null)
  const addressCopied = ref(false)

  const doctorOpen = ref(false)
  const doctorReport = ref<DoctorReport | null>(null)
  const doctorLoading = ref(false)
  const doctorError = ref<string | null>(null)
  const doctorCopied = ref(false)

  /**
   * 四种组合的判据：**只**看「服务管理器说注册了吗」+「控制面说活着吗」。
   * 与 Rust 侧 `Snapshot::phase_of` 同一套（不能靠「端口连不上」推断：前台运行时端口是通的）。
   */
  function derivePhase(current: ServiceStatus | null, process: ProcessStatus | null): Phase {
    if (!sidecarPresent.value) return 'incomplete'
    if (current?.installed === true) return process ? 'running' : 'installedStopped'
    return process ? 'foreground' : 'notInstalled'
  }

  // ---- 应用状态 ----

  function applySnapshot(next: Snapshot) {
    phase.value = next.phase
    service.value = next.service
    processStatus.value = next.process
    controlError.value = next.controlError
    discovery.value = next.discovery
    discoveryDetail.value = next.discoveryDetail
    relayUrl.value = next.relayUrl
    urlSource.value = next.urlSource
    coreBound.value = next.coreBound
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
    service.value = next
    phase.value = derivePhase(next, processStatus.value)
  }

  function applyDetached(error: CommandError) {
    // 心跳失败不是「错误弹窗」：卡片自己会变成「已安装，未运行」，这里只是把进程状态清掉
    processStatus.value = null
    controlError.value = error.message
    phase.value = derivePhase(service.value, null)
  }

  // ---- 提示条 ----

  function messageFor(error: CommandError): string {
    // 错误码 → 人话，别把原生错误码丢给用户（§ 3.1 / § 7）
    switch (error.code) {
      case 'sidecarMissing':
        return noticeText.sidecarMissing
      case 'versionMismatch':
        return noticeText.versionMismatch
      case 'controlUnreachable':
        return noticeText.controlUnreachable(error.message)
      default:
        return error.message
    }
  }

  function doctorAction(): NoticeAction {
    return { label: noticeText.needDoctor, kind: 'doctor' }
  }

  function noticeFromError(error: CommandError): Notice {
    // 提权被取消**不是错误**：黄条，不写红字（验收 #4）
    if (error.code === 'elevationCancelled') {
      return { tone: 'warn', text: noticeText.cancelledElevation }
    }
    return { tone: 'error', text: messageFor(error), action: doctorAction() }
  }

  /**
   * 状态本身带来的提示（不是某次操作的结果）。
   * § 5：服务管理器说「在跑」但控制面连不上 —— 多半正在重启 / 更新，说清楚别让用户以为坏了。
   */
  const statusNotice = computed<Notice | null>(() => {
    if (service.value?.running !== true || processStatus.value !== null) return null
    return {
      tone: 'warn',
      text: noticeText.controlUnreachable(controlError.value ?? discoveryDetail.value),
    }
  })

  /** 操作结果优先于状态提示。 */
  const visibleNotice = computed<Notice | null>(() => notice.value ?? statusNotice.value)

  function runNoticeAction() {
    const action = notice.value?.action
    notice.value = null
    if (action?.kind === 'doctor') openDoctor()
    else void refresh()
  }

  // ---- 拉状态 / 轮询（只在窗口可见时跑） ----

  let timer: ReturnType<typeof setTimeout> | undefined
  let pollCount = 0
  let failures = 0

  async function refresh() {
    try {
      applySnapshot(await ipc.snapshot())
      pollCount = 0
      failures = 0
    } catch (error) {
      // 读状态本身失败：给原始原因 + 诊断入口；已经有提示时别覆盖它（§ 5）
      if (notice.value === null) notice.value = noticeFromError(ipc.asCommandError(error))
    } finally {
      loading.value = false
    }
  }

  function clearTimer() {
    if (timer !== undefined) {
      clearTimeout(timer)
      timer = undefined
    }
  }

  function schedule(delay: number) {
    clearTimer()
    if (document.visibilityState !== 'visible') return
    timer = setTimeout(() => void tick(), delay)
  }

  async function tick() {
    timer = undefined
    if (document.visibilityState !== 'visible') return
    pollCount += 1
    try {
      const beat = await ipc.heartbeat()
      failures = 0
      applyHeartbeat(beat)
      if (pollCount % SERVICE_STATUS_EVERY === 0) {
        try {
          applyService(await ipc.serviceStatus())
        } catch {
          // 服务管理器查不动就沿用上一次的结果，不打扰用户
        }
      }
    } catch (error) {
      failures += 1
      applyDetached(ipc.asCommandError(error))
    } finally {
      schedule(failures >= FAILURES_BEFORE_BACKOFF ? POLL_BACKOFF_MS : POLL_MS)
    }
  }

  function onVisibilityChange() {
    if (document.visibilityState === 'visible') {
      // 回到前台：立刻刷新并恢复正常节奏
      pollCount = 0
      failures = 0
      void refresh()
      schedule(POLL_MS)
    } else {
      clearTimer()
    }
  }

  function onWindowFocus() {
    pollCount = 0
    failures = 0
    void refresh()
    schedule(POLL_MS)
  }

  onMounted(() => {
    void refresh()
    document.addEventListener('visibilitychange', onVisibilityChange)
    window.addEventListener('focus', onWindowFocus)
    schedule(POLL_MS)
  })

  onBeforeUnmount(() => {
    document.removeEventListener('visibilitychange', onVisibilityChange)
    window.removeEventListener('focus', onWindowFocus)
    clearTimer()
  })

  // ---- 操作 ----

  function applyResult(result: ActionResult) {
    if (result.cancelled) {
      // 取消提权不是错误：黄条，状态不变，不产生「错误」记录（验收 #4）
      notice.value = { tone: 'warn', text: noticeText.cancelledElevation }
      return
    }
    if (!result.ok) {
      notice.value = { tone: 'error', text: result.message, action: doctorAction() }
      return
    }
    // 成功不弹对话框：卡片刷新本身就是反馈（§ 3.1）
    notice.value = null
  }

  /**
   * 操作的通用流程（§ 3.1）：立即进入「进行中」→ await → **失败也要重新拉状态**
   * → 结束「进行中」+ 显示结果。
   *
   * `mode` 只有「安装为系统服务」会传：用来区分进行中那条文案。
   */
  async function runGuarded(
    target: BusyTarget,
    work: () => Promise<ActionResult>,
    mode: ServiceLevel | null = null,
  ) {
    if (busy.value !== null) return
    notice.value = null
    busy.value = target
    pendingMode.value = mode

    const watchdog = setTimeout(() => {
      // 不取消调用（取消可能把服务留在半路），只提示「可能还在进行」
      notice.value = {
        tone: 'warn',
        text: noticeText.timeout,
        action: { label: noticeText.timeoutAction, kind: 'refresh' },
      }
    }, ACTION_TIMEOUT_MS)

    try {
      applyResult(await work())
    } catch (error) {
      notice.value = noticeFromError(ipc.asCommandError(error))
    } finally {
      clearTimeout(watchdog)
      pendingMode.value = null
      await refresh()
      busy.value = null
    }
  }

  function runAction(name: ServiceActionName) {
    void runGuarded(name, () => ipc.serviceAction(name))
  }

  function installAsSystemService() {
    void runGuarded(
      'install',
      () => ipc.serviceAction('install', { mode: 'system', autostart: true }),
      'system',
    )
  }

  function toggleAutostart(on: boolean) {
    void runGuarded('autostart', () => ipc.setAutostart(on))
  }

  // ---- 可用性矩阵（§ 3「何时可用」） ----

  /** 安装包不完整：所有操作都禁用（§ 5）。 */
  const allBlocked = computed(() => phase.value === 'incomplete')

  const availability = computed<Record<ServiceActionName, boolean>>(() => {
    const installed = service.value?.installed === true
    const running = processStatus.value !== null
    return {
      install: !installed,
      uninstall: installed,
      start: installed && !running,
      stop: running,
      restart: running,
    }
  })

  function canRun(name: ServiceActionName): boolean {
    if (allBlocked.value || loading.value || busy.value !== null) return false
    return availability.value[name]
  }

  /** 禁用原因：先排掉全局原因，再看这个操作在当前状态下的原因。 */
  function actionReason(name: ServiceActionName): string {
    if (busy.value !== null) return misc.busy
    if (loading.value) return misc.loading
    if (allBlocked.value) return noticeText.sidecarMissing
    return disabledReasonByPhase[phase.value][name] ?? disabledReason[name] ?? misc.actionNotAvailable
  }

  /** 主按钮用主色；卸载是破坏性的，永远用红色提醒。 */
  function variantOf(name: ServiceActionName, primary: ServiceActionName | null): ActionItem['variant'] {
    if (name === primary) return 'primary'
    if (name === 'uninstall') return 'danger'
    return 'secondary'
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
    if (allBlocked.value) return noticeText.sidecarMissing
    return misc.autostartDisabled
  })

  /** 中继没在跑就不显示它的版本号（验收 #12）。 */
  const versionText = computed(() =>
    infoText.versionValue(processStatus.value?.version, coreBound.value),
  )

  // ---- 诊断 ----

  async function loadDoctor() {
    doctorLoading.value = true
    doctorError.value = null
    try {
      doctorReport.value = await ipc.doctor()
    } catch (error) {
      doctorReport.value = null
      doctorError.value = messageFor(ipc.asCommandError(error))
    } finally {
      doctorLoading.value = false
    }
  }

  function setDoctorOpen(open: boolean) {
    doctorOpen.value = open
    // 展开时没有结果就现拉一次（上次拉失败的话，再展开会重试）
    if (open && doctorReport.value === null && !doctorLoading.value) void loadDoctor()
  }

  function openDoctor() {
    setDoctorOpen(true)
  }

  /** 诊断结果转纯文本（贴 issue 用）。 */
  function doctorAsText(report: DoctorReport): string {
    return report.checks
      .map((check) => {
        const head = `${doctorText.mark[check.level]} ${check.title}${
          check.detail === '' ? '' : `\t${check.detail}`
        }`
        return check.action ? `${head}\n\t${doctorText.suggestion}${check.action}` : head
      })
      .join('\n')
  }

  // ---- 复制 / 打开目录 ----

  async function copyAddress() {
    if (relayUrl.value === '') return
    try {
      // 复制的必须是发现文件里那个地址本身，一个字都不加工（验收 #7）
      await ipc.copyText(relayUrl.value)
      addressCopied.value = true
      setTimeout(() => {
        addressCopied.value = false
      }, COPIED_MS)
    } catch (error) {
      notice.value = noticeFromError(ipc.asCommandError(error))
    }
  }

  async function copyDoctor() {
    const report = doctorReport.value
    if (report === null) return
    try {
      await ipc.copyText(doctorAsText(report))
      doctorCopied.value = true
      setTimeout(() => {
        doctorCopied.value = false
      }, COPIED_MS)
    } catch (error) {
      notice.value = noticeFromError(ipc.asCommandError(error))
    }
  }

  async function openConfigDir() {
    try {
      await ipc.openConfig()
    } catch (error) {
      notice.value = noticeFromError(ipc.asCommandError(error))
    }
  }

  async function openLogsDir() {
    try {
      await ipc.openLogs()
    } catch (error) {
      notice.value = noticeFromError(ipc.asCommandError(error))
    }
  }

  return {
    phaseTitle: computed(() => phaseText[phase.value].title),
    phaseSubtitle: computed(() =>
      phaseText[phase.value].subtitle({
        process: processStatus.value,
        service: service.value ?? {},
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
