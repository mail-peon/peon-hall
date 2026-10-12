// 与 useRelayState 配套的**纯函数**：不读 ref、不挂 vue 副作用，
// 单测可以直接 import 跑（`src/composables/__tests__/relay-pure.spec.ts`）。
//
// 抽出来的原则：判据逻辑 / 状态组合 / 文案映射都在这里，
// useRelayState 只负责「拼装 ref + 编排副作用 + 把纯函数的结果塞进 ref」。

import type {
  CommandError,
  DoctorReport,
  Notice,
  Phase,
  ProcessStatus,
  ServiceActionName,
  ServiceStatus,
} from '../types'

/** 状态组合的判据：见 `useRelayState.derivePhase` 的同款注释。 */
export function phaseOf(
  sidecarPresent: boolean,
  serviceError: string | null,
  service: ServiceStatus | null,
  process: ProcessStatus | null,
): Phase {
  if (!sidecarPresent)
    return 'incomplete'
  if (serviceError !== null)
    return 'serviceUnknown'
  if (service?.installed === true)
    return process ? 'running' : 'installedStopped'
  return process ? 'foreground' : 'notInstalled'
}

/** 操作可用性矩阵（与 `useRelayState.availability` 同款）。 */
export function availabilityFor(
  installed: boolean,
  running: boolean,
): Record<ServiceActionName, boolean> {
  return {
    install: !installed,
    uninstall: installed,
    start: installed && !running,
    stop: running,
    restart: running,
  }
}

/** 主按钮主色；卸载永远是红色。 */
export function variantOf(
  name: ServiceActionName,
  primary: ServiceActionName | null,
): 'primary' | 'danger' | 'secondary' {
  if (name === primary)
    return 'primary'
  if (name === 'uninstall')
    return 'danger'
  return 'secondary'
}

/** 错误码 → 人话（不要把原生错误码丢给用户）。 */
export function messageFor(error: CommandError, texts: { sidecarMissing: string, versionMismatch: string, controlUnreachable: (detail: string) => string }): string {
  switch (error.code) {
    case 'sidecarMissing':
      return texts.sidecarMissing
    case 'versionMismatch':
      return texts.versionMismatch
    case 'controlUnreachable':
      return texts.controlUnreachable(error.message)
    default:
      return error.message
  }
}

/** 错误 → 提示条对象（黄条 = 取消提权，红条 = 其他）。 */
export function noticeFromError(
  error: CommandError,
  texts: {
    cancelledElevation: string
    needDoctor: string
    sidecarMissing: string
    versionMismatch: string
    controlUnreachable: (detail: string) => string
  },
): Notice {
  if (error.code === 'elevationCancelled') {
    return { tone: 'warn', text: texts.cancelledElevation }
  }
  return {
    tone: 'error',
    text: messageFor(error, texts),
    action: { label: texts.needDoctor, kind: 'doctor' },
  }
}

/** 诊断结果 → 一段纯文本（贴 issue 用）。 */
export function doctorAsText(
  report: DoctorReport,
  texts: { mark: { ok: string, warn: string, fail: string }, suggestion: string },
): string {
  return report.checks
    .map((check) => {
      const head = `${texts.mark[check.level]} ${check.title}${
        check.detail === '' ? '' : `\t${check.detail}`
      }`
      return check.action ? `${head}\n\t${texts.suggestion}${check.action}` : head
    })
    .join('\n')
}