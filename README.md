# peon-hall

> `mail-peon` IMAP 中继的**服务安装器 GUI**：一个 Tauri 2 桌面应用，
> 用来显示服务状态、安装/卸载服务、启动/停止服务、开关开机自启、渲染诊断结果。

![peon-hall 概念图：中继的服务安装器 GUI](.github/images/great-hall.png)

它**不含中继逻辑**：中继是另一个仓库里的独立二进制
（`mail-peon/peon-burrow`），本应用把它当作 sidecar 打包进来，
并在需要时以提权方式调用它的 `service` 子命令。

```
┌──────────────────────────────┐        控制面（命名管道 / Unix socket / loopback TCP）
│ 本应用（Tauri GUI）           │ ─────────────────────────────────▶  core 中继（运行中）
│ 状态卡片 · 5 个操作 · 诊断     │ ◀─────────────────────────────────  状态 / 端口 / 版本 / doctor
└──────────────┬───────────────┘
               │ 提权 spawn（安装/卸载服务、启停、自启开关）
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

| 项 | 状态 |
| --- | --- |
| 界面规格与状态机 | ✅ [`ai-docs/01-ui-and-states.md`](./ai-docs/01-ui-and-states.md) |
| 与 core 的集成方式（控制面 + 提权路径） | ✅ [`ai-docs/02-core-integration.md`](./ai-docs/02-core-integration.md) |
| sidecar 打包与构建 | ✅ [`ai-docs/03-build-and-sidecar.md`](./ai-docs/03-build-and-sidecar.md) |
| 发版流程 | ✅ [`ai-docs/04-release.md`](./ai-docs/04-release.md) |
| 前端（Vue 3 + Vite + TS） | ✅ `src/` |
| Rust 侧（控制面 / sidecar / 提权 / 命令） | ✅ `src-tauri/src/` |
| CI / 发版工作流 | ✅ `.github/workflows/` |
| 三平台安装包 + 干净机器人工验收 | ⬜ 需要打 tag 与真机（`ai-docs/00-overview.md § 6` 七步） |

**界面规格在 peon-burrow 仓库有对应的决策依据**：
`ai-docs/decisions/adr-0006-desktop-installer.md`（形态、提权边界、为什么不做自更新）。

---

## 关键约束（先记住这四条）

1. **不做自更新**：免安装工具，用户重装新安装包即可。不引入 `tauri-plugin-updater`，
   不配 `plugins.updater`，不生成 `latest.json`（CI 里有断言守着）。
2. **不实现服务注册**：安装/卸载/自启全部转发给 core 二进制的 `service` 子命令。
3. **状态有两个来源**：服务管理器（是否安装/自启）× 控制面（是否在跑/端口/版本）。
   四种组合要能分别显示，判定逻辑在 `src-tauri/src/snapshot.rs`（带单测）。
4. **主路径零 UAC**：默认装成「用户级自启」，不需要管理员权限；只有用户主动选「系统服务」
   时才提权（取消提权显示「已取消」，不是错误）。

---

## 开发

```bash
pnpm install

# 前端单独跑（浏览器里能看到界面骨架，但 Tauri 命令会失败 —— 它只在 webview 里有）
pnpm dev

# 完整桌面应用
pnpm tauri dev

# 类型检查 / 构建前端 / 类型对拍
pnpm typecheck
pnpm build
node scripts/check-types.mjs
```

### 本地联调（两个仓库并排）

⚠️ **`src-tauri/Cargo.toml` 里的 `peon-burrow-ipc` 现在是 `path` 依赖**，
指向 `../../peon-burrow/crates/peon-burrow-ipc` —— 这样改 core 立刻在桌面端生效，
调试最省事。**发版前必须换成钉死的 git tag 依赖（或 crates.io 版本依赖）**，
否则 CI 与别人的 checkout 都找不到那个路径（`ai-docs/02-core-integration.md § 1`）。

```bash
# 1) 编译 core 并把它的二进制放到 Tauri 要的位置（命名契约见 ai-docs/03-build-and-sidecar.md § 2）
cd ../peon-burrow && cargo build -p peon-burrow
cd ../peon-hall && pwsh scripts/sync-sidecar.ps1      # Windows
#                ./scripts/sync-sidecar.sh            # Linux / macOS

# 2) 起中继（前台，方便看日志）
cd ../peon-burrow && ./target/debug/burrow run

# 3) 起桌面端
cd ../peon-hall && pnpm tauri dev
```

| 脚本 | 用途 |
| --- | --- |
| `scripts/sync-sidecar.ps1` / `.sh` | 开发期：把本地 core 二进制拷成 `src-tauri/binaries/burrow-<triple>[.exe]` |
| `scripts/fetch-sidecar.sh` | CI/发版：从 core release 下载并**校验 sha256** 后放到同一位置 |
| `scripts/bump-version.mjs` | `pnpm version:bump 0.1.1`：一次改 `package.json` + `tauri.conf.json` + `src-tauri/Cargo.toml` |
| `scripts/check-types.mjs` | 对拍前端类型与 Rust 字段名（防「界面状态全是空的」） |

`src-tauri/binaries/` **不入库**：发版时用的二进制必须来自 core release 且校验过 sha256。

## License

[MIT](./LICENSE)
