# peon-hall

> `mail-peon` IMAP 中继的**服务安装器 GUI**：一个 Tauri 2 桌面应用，
> 用来显示服务状态、安装/卸载服务、启动/停止服务、开关开机自启。

它**不含中继逻辑**：中继是另一个仓库里的独立二进制
（`mail-peon/peon-burrow`），本应用把它当作 sidecar 打包进来，
并在需要时以提权方式调用它的 `service` 子命令。

```
┌──────────────────────────────┐        控制面（本地 socket / loopback）
│ 本应用（Tauri GUI）           │ ─────────────────────────────────▶  core 服务（运行中）
│ 状态卡片 · 5 个操作 · 诊断     │ ◀─────────────────────────────────  状态 / 端口 / 版本
└──────────────┬───────────────┘
               │ 提权 spawn（安装/卸载服务、自启开关）
               ▼
        core 二进制的 service 子命令（唯一实现服务注册的地方）
```

---

## 两个仓库

| 目录（本地并排） | GitHub | 是什么 |
| --- | --- | --- |
| **本仓库** | `mail-peon/peon-hall` | Tauri 2 安装器（本仓库） |
| `../peon-burrow` | `mail-peon/peon-burrow` | 中继核心 + 服务 + CLI + 自更新 |

父目录 `peon/` **不是仓库**，只是放两个 checkout 的容器。
两者怎么共享类型、怎么交付二进制、tag 怎么排：见 peon-burrow 仓库的
`ai-docs/decisions/adr-0001-two-repos.md`（本地 [`../peon-burrow/ai-docs/decisions/adr-0001-two-repos.md`](../peon-burrow/ai-docs/decisions/adr-0001-two-repos.md)）。

---

## 当前状态

> **文档先行阶段：代码尚未开始写。**

| 项 | 状态 |
| --- | --- |
| 界面规格与状态机 | ✅ [`ai-docs/01-ui-and-states.md`](./ai-docs/01-ui-and-states.md) |
| 与 core 的集成方式（控制面 + 提权路径） | ✅ [`ai-docs/02-core-integration.md`](./ai-docs/02-core-integration.md) |
| sidecar 打包与构建 | ✅ [`ai-docs/03-build-and-sidecar.md`](./ai-docs/03-build-and-sidecar.md) |
| 发版流程 | ✅ [`ai-docs/04-release.md`](./ai-docs/04-release.md) |
| 代码 | ⬜ |

**界面规格在 peon-burrow 仓库有对应的决策依据**：
`ai-docs/decisions/adr-0006-desktop-installer.md`（形态、提权边界、为什么不做自更新）。

---

## 关键约束（先记住这四条）

1. **不做自更新**：免安装工具，用户重装新安装包即可。不引入 `tauri-plugin-updater`，
   不配 `plugins.updater`，不生成 `latest.json`。
2. **不实现服务注册**：安装/卸载/自启全部转发给 core 二进制的 `service` 子命令。
3. **状态有两个来源**：服务管理器（是否安装/自启）× 控制面（是否在跑/端口/版本）。
   四种组合要能分别显示，见 `ai-docs/01-ui-and-states.md § 2`。
4. **主路径零 UAC**：默认装成「用户级自启」，不需要管理员权限；只有用户主动选「系统服务」
   或点「安装服务（系统级）」时才提权。

---

## 计划中的用法

```bash
# 本地开发（需要 ../peon-burrow 与 peon-burrow 的 release 资产）
pnpm install
pnpm tauri dev

# 构建安装包
pnpm tauri build
```

## License

[MIT](./LICENSE)
