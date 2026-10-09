// 界面上的**全部**文案（集中在这一个文件里，将来加语言只加一个文件）。
//
// 规则沿用 core：**说人话 / 给下一步 / 不甩锅 / 可复制**
// （peon-burrow 仓库 ai-docs/design/logging-and-diagnostics.md § 4）。
// 反面例子见 ai-docs/01-ui-and-states.md § 7：不要出现 `bind failed: EADDRINUSE` 这种文案。

import type { Autostart, Phase, ServiceActionName, ServiceLevel } from '../types'

/** 主状态卡片：标题 + 副标题 + 主次按钮。 */
export const phaseText: Record<
  Phase,
  {
    title: string
    /** 副标题可能用到运行数据，所以给一个函数。 */
    subtitle: (context: PhaseContext) => string
    primary: ServiceActionName | null
    secondary: ServiceActionName | null
    /** 状态色变量名（styles/main.css 里定义）。 */
    tone: string
  }
> = {
  running: {
    title: '正在运行',
    subtitle: ({ process }) =>
      process
        ? `端口 ${process.port} · 中继 v${process.version} · ${process.connections} 个监听`
        : '中继正在运行',
    primary: 'stop',
    secondary: 'restart',
    tone: 'running',
  },
  installedStopped: {
    title: '已安装，未运行',
    subtitle: ({ service }) =>
      service.lastExitCode === 5
        ? '刚刚更新完，正在重启中'
        : service.lastExitCode !== undefined
          ? `上次退出码 ${service.lastExitCode}`
          : '服务没有在运行。点【启动】。',
    primary: 'start',
    secondary: 'uninstall',
    tone: 'stopped',
  },
  foreground: {
    title: '前台运行中',
    subtitle: () => '未安装为服务（重启机器后不会自动运行）',
    primary: 'install',
    secondary: 'stop',
    tone: 'foreground',
  },
  notInstalled: {
    title: '未安装',
    subtitle: () => '安装后即可在扩展里收邮件',
    primary: 'install',
    secondary: null,
    tone: 'none',
  },
  incomplete: {
    title: '安装不完整：缺少中继程序',
    subtitle: () => '重新安装 peon-hall 即可修复',
    primary: null,
    secondary: null,
    tone: 'none',
  },
}

/** 拼副标题时要用的数据。 */
export interface PhaseContext {
  process: { port: number; version: string; connections: number } | null
  service: { lastExitCode?: number }
}

/** 五个操作。 */
export const actionText: Record<
  ServiceActionName,
  { label: string; busy: string; done: string }
> = {
  install: { label: '安装服务', busy: '正在安装…', done: '已安装' },
  uninstall: { label: '卸载服务', busy: '正在卸载…', done: '已卸载' },
  start: { label: '启动', busy: '正在启动…', done: '已启动' },
  stop: { label: '停止', busy: '正在停止…', done: '已停止' },
  restart: { label: '重启', busy: '正在重启…', done: '已重启' },
}

/** 「为什么这个按钮不能点」——禁用时必须给原因（不能只灰掉）。 */
export const disabledReason: Partial<Record<ServiceActionName, string>> = {
  stop: '中继没有在运行',
  restart: '中继没有在运行',
  uninstall: '服务还没安装',
  start: '服务没有在运行',
}

/** 按状态给「为什么不能点」：同一个按钮在不同状态下原因不同（上表是所有状态的兜底）。 */
export const disabledReasonByPhase: Record<
  Phase,
  Partial<Record<ServiceActionName, string>>
> = {
  running: {
    install: '服务已经装好了，不用重复安装',
    start: '中继已经在运行，不用再启动',
  },
  installedStopped: {
    install: '服务已经装好了，不用重复安装',
    stop: '中继没有在运行',
    restart: '中继没有在运行，先点【启动】',
  },
  foreground: {
    uninstall: '还没装成服务，用不着卸载',
    start: '中继已经在前台跑着了',
  },
  notInstalled: {
    uninstall: '服务还没安装，用不着卸载',
    start: '服务还没安装，先点【安装服务】',
    stop: '服务还没安装',
    restart: '服务还没安装，先点【安装服务】',
  },
  // 安装不完整时五个操作全禁用，原因统一用 notice.sidecarMissing
  incomplete: {},
}

/** 高级选项：装成系统服务（默认的用户级安装不需要管理员权限）。 */
export const advanced = {
  title: '高级：安装为系统服务',
  hint: '系统服务在所有人登录时都能收信，代价是安装 / 启动 / 停止都要管理员权限（会弹一次授权框）。日常使用用上面的【安装服务】就够了。',
  installSystem: '安装为系统服务',
  installSystemBusy: '正在安装系统服务…',
}

/** 附加信息行。 */
export const info = {
  autostart: '开机自启',
  autostartValue: (value: Autostart | undefined): string => {
    switch (value) {
      case 'logon':
        return '登录时启动'
      case 'boot':
        return '开机启动'
      case 'off':
        return '已关闭'
      default:
        return '—'
    }
  },
  level: '安装级别',
  levelValue: (value: ServiceLevel | undefined): string => {
    switch (value) {
      case 'user':
        return '用户级（不需要管理员）'
      case 'system':
        return '系统服务'
      default:
        return '—'
    }
  },
  relayAddress: '扩展地址',
  version: '版本',
  versionValue: (version: string | undefined, coreBound: string): string =>
    version ? `v${version}（桌面端绑定 ${coreBound}）` : `桌面端 v${coreBound === '' ? '—' : ''}`,
  urlSource: {
    controlPlane: '来自运行中的中继',
    configFile: '来自配置文件',
    default: '默认值（还没读到配置）',
  },
  /** 未安装时**不显示**中继版本号，避免误导（验收 #12）。 */
  unknown: '—',
}

/** 顶部提示条。 */
export const notice = {
  cancelledElevation: '已取消（安装系统服务需要管理员权限）',
  cancelled: '已取消',
  timeout: '操作超时，服务可能仍在启动中',
  timeoutAction: '刷新状态',
  running: (action: string) => `${action}`,
  needDoctor: '查看诊断',
  discoveryStale: '发现文件是旧的：中继可能正在重启或被停掉',
  controlUnreachable: (detail: string) => `连不上中继（可能正在重启或更新中）。${detail}`,
  sidecarMissing: '安装不完整：缺少中继程序。请重新安装 peon-hall',
  versionMismatch: '中继的返回看不懂：可能是版本不匹配，请更新 peon-hall',
  updating: '正在更新并重启中继…（扩展会在几秒内自动重连）',
}

/** 诊断视图。 */
export const doctorText = {
  title: '诊断',
  loading: '正在读取诊断结果…',
  failed: '读不到诊断结果',
  copyAll: '复制全部',
  copied: '已复制',
  openConfig: '打开配置',
  openLogs: '打开日志',
  refresh: '重新检查',
  /** 检查项等级前缀（✅ / ⚠️ / ❌）。 */
  mark: { ok: '✅', warn: '⚠️', fail: '❌' } as const,
  /** core 的 `action` 字段是**人话建议**（自由文本），按原样显示。 */
  suggestion: '建议：',
  copyUnavailable: '还没有诊断结果',
}

/** 其它零碎文案。 */
export const misc = {
  copyAddress: '复制扩展地址',
  copied: '已复制到剪贴板',
  refresh: '刷新状态',
  environment: '环境信息',
  offline: '离线也能用：本应用不会联网检查任何东西',
  trayHint: '关闭窗口不影响收信 —— 中继是后台服务',
  /** 有操作在进行中时，其它按钮的禁用原因。 */
  busy: '有操作正在进行中，请稍等',
  loading: '正在读取当前状态…',
  actionNotAvailable: '当前状态用不到这个操作',
  noAddress: '还没读到扩展地址',
  autostartDisabled: '把中继装成服务后才能设置开机自启',
}
