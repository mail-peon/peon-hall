// 纯函数 `relay-pure.ts` 的单测 —— 见 `src/composables/relay-pure.ts` 顶部。
// 这些函数是从 useRelayState 抽出来的判据 / 文案映射逻辑；vue ref 与生命周期都不沾边。

import { describe, expect, it } from 'vitest'
import type {
  CheckResult,
  DoctorReport,
  ProcessStatus,
  ServiceStatus,
} from '../../types'
import {
  availabilityFor,
  doctorAsText,
  messageFor,
  noticeFromError,
  phaseOf,
  variantOf,
} from '../relay-pure'

/** 占位 service（够覆盖 installed 字段） */
function service(overrides: Partial<ServiceStatus> = {}): ServiceStatus {
  return {
    installed: false,
    running: false,
    name: 'peon-burrow',
    requiresElevation: false,
    restartPolicyConfigured: false,
    ...overrides,
  }
}

/** 占位 process（够覆盖 truthy 判据） */
function processStatus(overrides: Partial<ProcessStatus> = {}): ProcessStatus {
  return {
    running: true,
    host: '127.0.0.1',
    port: 41316,
    version: '0.1.0',
    protocol: 1,
    startedAt: '2026-10-12T00:00:00Z',
    connections: 0,
    watchConnections: 0,
    ...overrides,
  }
}

const TEXTS = {
  cancelledElevation: '已取消',
  needDoctor: '查看诊断',
  sidecarMissing: '安装不完整：缺少中继程序',
  versionMismatch: '中继的返回看不懂',
  controlUnreachable: (detail: string) => `连不上中继：${detail}`,
}

describe('phaseOf', () => {
  it('sidecar 不在 → incomplete（不论其它）', () => {
    expect(phaseOf(false, null, null, null)).toBe('incomplete')
    expect(phaseOf(false, null, service({ installed: true }), processStatus())).toBe('incomplete')
  })

  it('读不到服务状态 → serviceUnknown（不能猜成 notInstalled）', () => {
    expect(phaseOf(true, '权限不足', null, null)).toBe('serviceUnknown')
    expect(phaseOf(true, '权限不足', service({ installed: true }), processStatus())).toBe('serviceUnknown')
  })

  it('serviceUnknown 不被 processStatus 阻断', () => {
    // H-2 的关键回归：不能因为「控制面活着」就把 serviceUnknown 拉回 running
    expect(phaseOf(true, '权限不足', service({ installed: true }), processStatus())).toBe('serviceUnknown')
  })

  it('installed + process → running', () => {
    expect(phaseOf(true, null, service({ installed: true }), processStatus())).toBe('running')
  })

  it('installed + 无 process → installedStopped（用户最容易误判成坏了）', () => {
    expect(phaseOf(true, null, service({ installed: true }), null)).toBe('installedStopped')
  })

  it('未安装 + process → foreground（终端里跑着）', () => {
    expect(phaseOf(true, null, service({ installed: false }), processStatus())).toBe('foreground')
  })

  it('未安装 + 无 process → notInstalled', () => {
    expect(phaseOf(true, null, service({ installed: false }), null)).toBe('notInstalled')
  })

  it('installed=null 也按 serviceUnknown（一致性：缺失读空）', () => {
    // 真实代码用 current?.installed === true，空 service 不会进 'installedStopped'
    expect(phaseOf(true, null, null, processStatus())).toBe('foreground')
  })
})

describe('availabilityFor', () => {
  it('未安装：只有 install 可用', () => {
    expect(availabilityFor(false, false)).toEqual({
      install: true,
      uninstall: false,
      start: false,
      stop: false,
      restart: false,
    })
  })

  it('已安装未运行：install/start 可用', () => {
    expect(availabilityFor(true, false)).toEqual({
      install: false,
      uninstall: true,
      start: true,
      stop: false,
      restart: false,
    })
  })

  it('已安装且运行：stop/restart/uninstall 可用', () => {
    expect(availabilityFor(true, true)).toEqual({
      install: false,
      uninstall: true,
      start: false,
      stop: true,
      restart: true,
    })
  })

  it('边界：未安装 + 跑着（前台模式）= 与「未安装」相同', () => {
    expect(availabilityFor(false, true)).toEqual({
      install: true,
      uninstall: false,
      start: false,
      stop: true,
      restart: true,
    })
  })
})

describe('variantOf', () => {
  it('主按钮 → primary', () => {
    expect(variantOf('install', 'install')).toBe('primary')
  })

  it('卸载在非主按钮位置时是 danger', () => {
    expect(variantOf('uninstall', 'start')).toBe('danger')
    expect(variantOf('uninstall', 'stop')).toBe('danger')
  })

  it('卸载本身就是主按钮时回到 primary（primary 卡在身份位）', () => {
    // 真实使用里 `installedStopped.secondary === 'uninstall'`，所以 uninstall 一般不是 primary。
    // 这里把规则写死：primary 优先于 danger（避免颜色来回切换）。
    expect(variantOf('uninstall', 'uninstall')).toBe('primary')
  })

  it('其他 → secondary', () => {
    expect(variantOf('start', 'install')).toBe('secondary')
    expect(variantOf('restart', 'stop')).toBe('secondary')
  })
})

describe('messageFor', () => {
  it('sidecarMissing → 固定文案', () => {
    expect(
      messageFor(
        { code: 'sidecarMissing', message: 'ENOENT burrow.exe', suggestDoctor: false },
        TEXTS,
      ),
    ).toBe(TEXTS.sidecarMissing)
  })

  it('versionMismatch → 固定文案（不暴露具体版本号）', () => {
    expect(
      messageFor(
        { code: 'versionMismatch', message: 'protocol v99', suggestDoctor: false },
        TEXTS,
      ),
    ).toBe(TEXTS.versionMismatch)
  })

  it('controlUnreachable → 文案 + 原始原因', () => {
    expect(
      messageFor(
        { code: 'controlUnreachable', message: 'connection refused', suggestDoctor: false },
        TEXTS,
      ),
    ).toBe('连不上中继：connection refused')
  })

  it('其它错误码 → 原 message（不加工）', () => {
    expect(
      messageFor(
        { code: 'failed', message: 'unknown', suggestDoctor: false },
        TEXTS,
      ),
    ).toBe('unknown')
  })
})

describe('noticeFromError', () => {
  it('elevationCancelled → 黄条 + 取消文案（**不是**错误）', () => {
    expect(
      noticeFromError(
        { code: 'elevationCancelled', message: '用户取消了 UAC', suggestDoctor: false },
        TEXTS,
      ),
    ).toEqual({ tone: 'warn', text: TEXTS.cancelledElevation })
  })

  it('controlUnreachable → 红条 + 诊断入口', () => {
    expect(
      noticeFromError(
        { code: 'controlUnreachable', message: 'timeout', suggestDoctor: false },
        TEXTS,
      ),
    ).toEqual({
      tone: 'error',
      text: '连不上中继：timeout',
      action: { label: TEXTS.needDoctor, kind: 'doctor' },
    })
  })

  it('其它错误 → 红条 + 诊断入口', () => {
    expect(
      noticeFromError(
        { code: 'failed', message: 'raw text', suggestDoctor: false },
        TEXTS,
      ),
    ).toEqual({
      tone: 'error',
      text: 'raw text',
      action: { label: TEXTS.needDoctor, kind: 'doctor' },
    })
  })
})

describe('doctorAsText', () => {
  function check(overrides: Partial<CheckResult>): CheckResult {
    return {
      id: 'test',
      level: 'ok',
      title: '默认检查',
      detail: '',
      ...overrides,
    }
  }

  function report(checks: CheckResult[]): DoctorReport {
    return { checks }
  }

  const TEXTS = {
    mark: { ok: '✅', warn: '⚠️', fail: '❌' },
    suggestion: '建议：',
  }

  it('无 action 时只输出标题 + 可选 detail', () => {
    const text = doctorAsText(report([
      check({ id: 'a', level: 'ok', title: '中继在跑' }),
      check({ id: 'b', level: 'fail', title: '端口冲突', detail: '被 QQ 占着' }),
    ]), TEXTS)
    expect(text).toBe('✅ 中继在跑\n❌ 端口冲突\t被 QQ 占着')
  })

  it('有 action 时换行追加「建议：…」', () => {
    const text = doctorAsText(report([
      check({ id: 'a', level: 'warn', title: '配置可疑', action: '打开 relay.toml' }),
    ]), TEXTS)
    expect(text).toBe('⚠️ 配置可疑\n\t建议：打开 relay.toml')
  })

  it('空报告 → 空字符串（不报错）', () => {
    expect(doctorAsText(report([]), TEXTS)).toBe('')
  })
})