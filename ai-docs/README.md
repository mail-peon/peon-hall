# peon-hall — 开发文档

> 本目录是**本仓库**（桌面端 GUI）的文档。
> 中继本体的设计与决策在姊妹仓库 core：
> 本地 [`../../peon-burrow/ai-docs/`](../../peon-burrow/ai-docs/README.md) · GitHub `peon-burrow`。
>
> ⚠️ 跨仓库引用一律写「仓库名 + 路径」的**纯文本**，**并**给出本地相对链接：
> GitHub 上跨仓库链接会失效。本地路径基准：

| 从 | 到 peon-burrow 仓库 | 到 `mail-peon` 扩展仓库 |
| --- | --- | --- |
| `ai-docs/*.md` | `../../peon-burrow/ai-docs/…` | `../../../mail-peon/…` |
| `ai-docs/decisions/*.md` | `../../../peon-burrow/ai-docs/…` | `../../../../mail-peon/…` |
| `README.md`（仓库根） | `../peon-burrow/ai-docs/…` | `../../mail-peon/…` |

---

## 目录

| 文件 | 说明 |
| --- | --- |
| [00-overview.md](./00-overview.md) | 这个应用是什么、边界、目标与非目标 |
| [01-ui-and-states.md](./01-ui-and-states.md) | 界面规格：状态卡片、四个状态组合、5 个操作、反馈与错误文案 |
| [02-core-integration.md](./02-core-integration.md) | 与 core 的集成：控制面客户端、状态合成、提权调用路径 |
| [03-build-and-sidecar.md](./03-build-and-sidecar.md) | sidecar 打包契约、`tauri.conf.json`、capabilities、三平台构建 |
| [04-release.md](./04-release.md) | 发版 runbook：版本唯一来源、tag 校验、CI、产物 |
| [decisions/adr-1001-frontend-stack.md](./decisions/adr-1001-frontend-stack.md) | 前端技术栈选择（Vue 3 + Vite + TS） |
| [decisions/adr-1002-implementation-revisions.md](./decisions/adr-1002-implementation-revisions.md) | **落地时的偏差总表**（优先于被点名段落） |

**必须知道的边界**（细节在 peon-burrow 仓库的 ADR）：

| 事项 | 决策 | 依据（peon-burrow 仓库） |
| --- | --- | --- |
| 形态 | Tauri 2 应用，服务安装器 | `ai-docs/decisions/adr-0006-desktop-installer.md` |
| 自更新 | **不做** | 同上 § Q5 |
| 服务注册逻辑 | **不在本仓库** | 同上 § Q2 |
| 控制面协议 | core 的 `peon-burrow-ipc-types`（类型，契约）与 `peon-burrow-ipc`（传输/客户端），git 依赖 + tag 钉版 | `ai-docs/decisions/adr-0001-two-repos.md` § 决策 3 |
| 提权 | 提权 spawn core 的 `service` 子命令 | `ai-docs/decisions/adr-0006-desktop-installer.md` § Q3 |

---

## 当前状态

> 最近更新：**D0–D4 实现完成**；D5（诊断）与 D6（CI + 发版）也已落地，
> 只剩三平台安装包与干净机器的人工验收（`00-overview.md § 6` 七步）。

| 阶段 | 内容 | 状态 |
| --- | --- | --- |
| D0 | 文档（本目录） | ✅ |
| D1 | Tauri 骨架 + 状态卡片（读服务管理器 + 控制面） | ✅ `src-tauri/src/{snapshot,discovery,control}.rs` |
| D2 | 5 个操作（用户级，零 UAC） | ✅ `src-tauri/src/{sidecar,commands}.rs` + `src/components/ActionBar.vue` |
| D3 | sidecar 打包 + 从 core release 取二进制 | ✅ `scripts/{sync-sidecar,fetch-sidecar}.*` + `bundle.externalBin` |
| D4 | 提权路径（系统服务安装/卸载/自启） | ✅ `src-tauri/src/elevate.rs`（取消 ≠ 错误） |
| D5 | 诊断视图（`doctor --json`）+ 复制扩展地址 | ✅ `src/components/{DoctorPanel,RelayAddress}.vue` |
| D6 | CI 三平台安装包 + 首次发版 | 🟡 CI 已就绪；安装包与人工验收待做 |
