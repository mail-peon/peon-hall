import type { Ref } from 'vue'
import { ref } from 'vue'
import * as ipc from '../ipc'
import { notice as noticeText } from '../locales/zh-CN'
import type { ActionResult, BusyTarget, Notice, Phase, ServiceActionName, ServiceLevel } from '../types'
import { doctorAction, messageFor, noticeFromError } from './relay-feedback'

const ACTION_TIMEOUT_MS = 30_000

export function useRelayActions(deps: {
  notice: Ref<Notice | null>
  phase: Ref<Phase>
  serviceError: Ref<string | null>
  refresh: () => Promise<void>
}) {
  const { notice, phase, serviceError, refresh } = deps
  const busy = ref<BusyTarget | null>(null)
  const pendingMode = ref<ServiceLevel | null>(null)
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

  return { busy, pendingMode, runGuarded, runAction, installAsSystemService, toggleAutostart }
}
