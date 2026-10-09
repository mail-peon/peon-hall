// 前端用到的全部类型。**字段名必须与 Rust 侧一致**。
//
// ⚠️ 这里**不是**第二份真相：`ServiceStatus` / `ProcessStatus` / `DoctorReport` 的形状由
// `peon-burrow-ipc-types`（Rust）定义，`Snapshot` 等由 `src-tauri/src/commands.rs` 定义。
// 手写 TS 接口是为了不引 ts-rs/specta，代价是**必须**有对拍：
// `scripts/check-types.mjs` 会去 Rust 源码里逐个字段名核对，CI 里跑（ADR-1001 Q2）。

/** 界面主状态：「现在是什么状态」。 */
export type Phase =
  | 'running' // 已安装 + 控制面可连
  | 'installedStopped' // 已安装 + 连不上（用户最容易误判成「坏了」）
  | 'foreground' // 未安装 + 控制面可连（终端里跑着）
  | 'notInstalled' // 未安装 + 连不上
  | 'incomplete' // sidecar 不在（安装包损坏）

/** 扩展地址是从哪来的。 */
export type UrlSource = 'controlPlane' | 'configFile' | 'default'

/** 发现文件（`control.json`）的三态。 */
export type DiscoveryState = 'missing' | 'stale' | 'fresh'

/** 安装级别。 */
export type ServiceLevel = 'user' | 'system'

/** 自启方式。 */
export type Autostart = 'logon' | 'boot' | 'off'

/** 服务管理器看到的状态（来自 `burrow service status --json`）。 */
export interface ServiceStatus {
  installed: boolean
  running: boolean
  level?: ServiceLevel
  autostart?: Autostart
  name: string
  binaryPath?: string
  /** 提权操作需要管理员权限（由中继判定，界面不要自己猜）。 */
  requiresElevation: boolean
  /** 失败重启策略**真的**写进去了吗 —— 服务崩了能不能自己回来的唯一信号。 */
  restartPolicyConfigured: boolean
  lastExitCode?: number
}

/** 控制面看到的进程状态。 */
export interface ProcessStatus {
  running: boolean
  host: string
  port: number
  version: string
  protocol: number
  startedAt: string
  connections: number
  watchConnections: number
  lastError?: string
}

/** 一次完整快照（打开窗口 / 获得焦点 / 每次操作结束后拉）。 */
export interface Snapshot {
  phase: Phase
  service: ServiceStatus
  process: ProcessStatus | null
  controlError: string | null
  discovery: DiscoveryState
  discoveryDetail: string
  relayUrl: string
  urlSource: UrlSource
  sidecarPresent: boolean
  appVersion: string
  coreBound: string
}

/** 只含控制面那一半（2 秒轮询用，轻）。 */
export interface Heartbeat {
  process: ProcessStatus | null
  controlError: string | null
  discovery: DiscoveryState
  discoveryDetail: string
  relayUrl: string
  urlSource: UrlSource
}

/** 一次服务动作的结果。 */
export interface ActionResult {
  ok: boolean
  /** 用户取消了提权 —— **不是错误**，界面显示「已取消」。 */
  cancelled: boolean
  message: string
  output?: unknown
}

/** 应用自身信息。 */
export interface AppInfo {
  appVersion: string
  coreBound: string
  platform: string
}

/** 命令错误码（按它分支，**不要**匹配 message）。 */
export type ErrorCode =
  | 'elevationCancelled'
  | 'sidecarMissing'
  | 'controlUnreachable'
  | 'versionMismatch'
  | 'failed'

/** 命令错误。 */
export interface CommandError {
  code: ErrorCode
  message: string
  suggestDoctor: boolean
}

/** 诊断结论等级。 */
export type CheckLevel = 'ok' | 'warn' | 'fail'

/** 诊断里的单项检查（形状来自 core 的 `doctor`）。 */
export interface CheckResult {
  id: string
  level: CheckLevel
  title: string
  detail: string
  action?: string
}

/** 诊断结果。 */
export interface DoctorReport {
  checks: CheckResult[]
}

/** 五个操作。 */
export type ServiceActionName = 'install' | 'uninstall' | 'start' | 'stop' | 'restart'

/** 命令抽屉里一行内容的来源。 */
export type CommandEventKind = 'command' | 'stdout' | 'stderr' | 'exit'

/** 中继命令的一行事件（Rust 侧 `commands::CommandEvent`）。 */
export interface CommandEvent {
  /** 一次调用的序号：同一次动作的行共用一个 id。 */
  id: number
  kind: CommandEventKind
  /** `user` = 用户点的操作（会**自动弹出**抽屉）；`auto` = 界面自己的轮询。 */
  origin: 'user' | 'auto'
  /** 内容；`exit` 时是退出码。 */
  line: string
}
