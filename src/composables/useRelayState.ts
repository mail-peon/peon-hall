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
import {
  availabilityFor,
  doctorAsText as doctorAsTextPure,
  messageFor as messageForPure,
  noticeFromError as noticeFromErrorPure,
  phaseOf,
  variantOf as variantOfPure,
} from './relay-pure'
import type {
  ActionItem,
  ActionResult,
  BusyTarget,
  CommandError,
  DiscoveryState,
  DoctorReport,
  Heartbeat,
  Notice,
  NoticeAction,
  Phase,
  ProcessStatus,
  ServiceActionName,
  ServiceLevel,
  ServiceStatus,
  Snapshot,
  UrlSource,
} from '../types'

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

  function messageFor(error: CommandError): string {
    return messageForPure(error, noticeText)
  }

  function doctorAction(): NoticeAction {
    return { label: noticeText.needDoctor, kind: 'doctor' }
  }

  function noticeFromError(error: CommandError): Notice {
    return noticeFromErrorPure(error, noticeText)
  }

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

  function runNoticeAction() {
    const action = notice.value?.action
    notice.value = null
    if (action?.kind === 'doctor') openDoctor()
    else if (action?.kind === 'uninstall') {
      // H-7：撤销安装走的就是普通卸载（提权按当前安装级别），结果照常显示在提示条上
      void runGuarded('uninstall', () => ipc.serviceAction('uninstall'))
    } else void refresh()
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
      // 取消提权不是错误：黄条，状态不变，不产生「错误」记录（验收 #4）。
      // 文案用中继给的那句：它知道是哪一级、哪个动作（弹 UAC 的不只是「安装系统服务」，
      // 系统服务的【停止】在控制面连不上时降级也要一次授权 —— H-3）。
      notice.value = {
        tone: 'warn',
        text: result.message || noticeText.cancelledElevation,
      }
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
   * H-7：安装失败后的那条提示 —— 注册项可能已经留下了。
   *
   * 「已注册但没在跑」是最典型的半装状态（注册成功、启动或就绪自检失败）。
   * 这里**不自动反注册**：一次慢启动不该变成「装的东西没了」。给出事实 +
   * 【撤销安装】一个按钮，修还是撤由用户决定（`ai-docs/01-ui-and-states.md § 3.1`）。
   *
   * ⚠️ 必须在 `refresh()` **之后**调：判据是刷新后的状态，不是失败时的那句错误。
   */
  function offerInstallRollback(reason: string) {
    // 判据必须是**刚刚读到的**状态：读不到服务状态时不要凭旧状态劝用户撤销
    if (phase.value !== 'installedStopped' || serviceError.value !== null) return
    notice.value = {
      tone: 'error',
      text: noticeText.installLeftover(reason),
      action: { label: noticeText.undoInstall, kind: 'uninstall' },
    }
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

    // 失败了就记下那句人话：H-7 要拿它拼「注册项还在」的提示（提权取消不算失败）
    let failure: string | null = null
    try {
      const result = await work()
      if (!result.ok && !result.cancelled) failure = result.message
      applyResult(result)
    } catch (error) {
      const commandError = ipc.asCommandError(error)
      failure = messageFor(commandError)
      notice.value = noticeFromError(commandError)
    } finally {
      clearTimeout(watchdog)
      pendingMode.value = null
      await refresh()
      busy.value = null
      if (failure !== null && target === 'install') offerInstallRollback(failure)
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

  // ---- 诊断 ----

  async function loadDoctor() {
    doctorLoading.value = true
    doctorError.value = null
    try {
      // 两路并发：burrow 自己的 doctor + peon-hall 的扩展连通性检查
      const [coreReport, reachable] = await Promise.all([
        ipc.doctor(),
        ipc.checkExtensionReachable(relayUrl.value),
      ])
      doctorReport.value = {
        checks: [
          ...coreReport.checks,
          reachable,
        ],
      }
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
    return doctorAsTextPure(report, doctorText)
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
