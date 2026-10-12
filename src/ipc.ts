// 调 Tauri 命令的**薄封装**：不写业务逻辑、不做平台判断（那些都在 Rust 侧）。
//
// 前端只做渲染与交互 —— 见 ai-docs/00-overview.md § 5「职责边界」。

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'

import type {
  ActionResult,
  AppInfo,
  CheckResult,
  CommandEvent,
  CommandError,
  DoctorReport,
  Heartbeat,
  ServiceActionName,
  ServiceLevel,
  ServiceStatus,
  Snapshot,
} from './types'

/** Tauri 抛出来的错误可能是字符串，也可能是我们的结构化错误。 */
export function asCommandError(error: unknown): CommandError {
  if (typeof error === 'object' && error !== null && 'code' in error && 'message' in error) {
    return error as CommandError
  }
  return {
    code: 'failed',
    message: typeof error === 'string' ? error : String(error),
    suggestDoctor: true,
  }
}

/** 完整状态（窗口打开 / 获得焦点 / 操作结束后）。 */
export function snapshot(): Promise<Snapshot> {
  return invoke<Snapshot>('snapshot')
}

/** 只问控制面（2 秒轮询用；不碰服务管理器 —— 那是一次进程调用）。 */
export function heartbeat(): Promise<Heartbeat> {
  return invoke<Heartbeat>('heartbeat')
}

/** 只问服务管理器（每 5 次轮询一次）。
 *
 * ⚠️ 返回类型**不是** `Snapshot['service']`：那一个现在可以是 `null`（快照会用
 * `serviceError` 说明「这一轮没读到」），而这个命令读不到是**抛错**，不会返回 null。
 */
export function serviceStatus(): Promise<ServiceStatus> {
  return invoke<ServiceStatus>('service_status')
}

/** 五个操作之一。`mode` 只在安装/系统服务时才需要传。 */
export function serviceAction(
  action: ServiceActionName,
  options: { mode?: ServiceLevel; autostart?: boolean } = {},
): Promise<ActionResult> {
  return invoke<ActionResult>('service_action', {
    action,
    mode: options.mode ?? null,
    autostart: options.autostart ?? null,
  })
}

/** 开机自启开关。 */
export function setAutostart(on: boolean): Promise<ActionResult> {
  return invoke<ActionResult>('set_autostart', { on })
}

/** 一条白名单内的控制面命令（`doctor` / `version` / `restart` / `updateCheck` …）。 */
export function controlCommand<T = unknown>(
  command: string,
  payload?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>('control_command', { command, payload: payload ?? null })
}

/** 诊断（走控制面的 `doctor`）。 */
export function doctor(verbose = true): Promise<DoctorReport> {
  return controlCommand<DoctorReport>('doctor', { verbose })
}

/**
 * 扩展连通性诊断：模拟扩展侧的 TCP 探测，从桌面端进程对中继地址做一次握手。
 *
 * 与 `doctor()` 的差异（来自 `src-tauri/src/doctor.rs` 的注释）：
 * - `doctor()` 查的是**中继自己的健康**（配置 / 端口 / 日志），走 burrow 控制面；
 * - 这一查的是「扩展 webview 跑这个 `relayUrl` 能不能连上」—— 二者都失败时
 *   用户的诊断路径不一样，所以是分开的两项。
 */
export function checkExtensionReachable(relayUrl: string): Promise<CheckResult> {
  return invoke<CheckResult>('check_extension_reachable', { relayUrl })
}

/** 应用信息（版本、绑定的 core 版本）。 */
export function appInfo(): Promise<AppInfo> {
  return invoke<AppInfo>('app_info')
}

/** 用系统文件管理器打开配置目录。 */
export function openConfig(): Promise<string> {
  return invoke<string>('open_config')
}

/** 打开日志目录。 */
export function openLogs(): Promise<string> {
  return invoke<string>('open_logs')
}

/** 复制到剪贴板（capabilities 里只为它开了权限）。 */
export function copyText(text: string): Promise<void> {
  return writeText(text)
}

/** Rust 侧发命令行的通道名（与 `commands::COMMAND_EVENT` 必须一致）。 */
export const COMMAND_EVENT = 'peon-hall://command'

/**
 * 订阅「中继命令的一行」。
 *
 * 返回取消订阅的函数 —— 组件卸载时必须调，否则热重载会叠出多份监听。
 */
export async function onCommandEvent(handler: (event: CommandEvent) => void): Promise<() => void> {
  return listen<CommandEvent>(COMMAND_EVENT, (event) => handler(event.payload))
}
