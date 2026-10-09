# ADR-1001 · 前端技术栈：Vue 3 + Vite + TypeScript

> 编号从 **1001** 起：本仓库是独立仓库，自己的局部 ADR 用 1000+ 段，
> 避免与 peon-burrow 仓库的 `adr-0001…` 撞号（peon-burrow 仓库 `ai-docs/README.md` 的命名约定）。

- **状态**：已采纳
- **影响面**：`desktop/src`、构建配置、CI
- **相关**：peon-burrow 仓库 `ai-docs/decisions/adr-0006-desktop-installer.md`（为什么是 Tauri）

---

## 背景

界面很小：**一个状态卡片 + 五个按钮 + 一个诊断列表 + 一个可复制的地址**。
没有路由、没有多页、没有复杂表单、没有列表虚拟化。

但它是「用户唯一会看到的界面」，所以要在**迭代速度**与**长期可维护**之间取平衡。

## 问题

| # | 问题 |
| --- | --- |
| Q1 | 用什么前端框架/语言？ |
| Q2 | 要不要状态管理库（Pinia / Vuex）？ |
| Q3 | 要不要组件库（Element Plus / Naive UI / shadcn 风）？ |
| Q4 | 要不要路由？ |
| Q5 | 样式方案？ |

## 决策

### Q1：**Vue 3 + Vite + TypeScript**

| 备选 | 否决理由 |
| --- | --- |
| **纯 TS + 手写 DOM** | 界面虽小，但「状态 → 视图」的映射有 4 种组合 + 5 个按钮的可用性矩阵，手写 DOM 会把这个矩阵散落在 update 函数里 |
| React | 作者生态（`mail-peon`、`btools-vue`、`triggerix-editor-vue`…）是 Vue；引入第二套心智无收益 |
| Svelte / Solid | 体积更小，但生态与经验不占优 |
| **Vue 3 + Vite + TS** ✅ | 与作者现有项目一致（`mail-peon` 本身就是 Vue 3 + Vite + UnoCSS）；组合式 API 对这种「小状态机 + 视图」的场景最顺手；Vite 的 dev/HMR 体验好 |

配套：`@tauri-apps/api` v2、`vue-tsc` 做类型检查。

### Q2：**不要**状态管理库

状态总量：`{ serviceStatus, controlStatus, version, doctorResult, busyAction, error }`。
用 `ref` / `reactive` + 一两个 `computed` 足够。
引入 Pinia 只会让「状态从哪来」多一层间接。

⚠️ 但**类型必须来自 Rust 侧**（`peon-burrow-ipc` + 本仓库的 Tauri 命令），
不要在前端手写 JSON 接口的 interface —— 那是最容易漂移的地方。
（`ts-rs` / `specta` 之类的自动生成工具可选；首版用「Tauri 命令返回类型 + 手写 4 个窄接口」，
并在 `ci.yaml` 里加一步「对拍 Rust 侧的结构体字段名」。）

### Q3：**不要**组件库

五个按钮 + 一张卡片 + 一个列表。组件库会带来：
① 几 MB 的样式与依赖；② 与「尽量原生、跟随系统深色模式」的目标冲突；
③ 版本升级噪音。手写 CSS（约 200 行）+ `prefers-color-scheme` 足够。

### Q4：**不要**路由

单页。诊断视图用「展开/折叠」而不是「另一个路由」。

### Q5：**UnoCSS**（与作者其它项目一致）

`unocss` + `@unocss/preset-wind4`（或当前主版本）替代手写原子类；
非原子部分（卡片阴影、状态色变量）写在 `src/styles/` 里。
深色模式用 `dark:` 变体 + `prefers-color-scheme` 的媒体查询策略。

---

## 后果

### 好的

- 与作者既有项目同一套工具链 → 切换成本为零；
- 依赖极少（Vue + UnoCSS + Tauri API），安装包与冷启动都不受前端拖累；
- 状态少 → 不用架构，代码量可控制在千行以内。

### 代价（如实记录）

| 代价 | 说明 |
| --- | --- |
| 类型对拍要人工维护 | Rust 侧改了字段名，前端不一定编译报错 → 用 CI 的字段名断言兜住（Q2 的 ⚠️） |
| 手写 CSS | 4 个状态色 + 卡片布局要自己写；量小但要留意深色模式与高 DPI |
| 无组件库 | 后续若要加「设置页」之类，可能需要重新评估（见后续 2） |

### 否决的方案

| 方案 | 否决理由 |
| --- | --- |
| 纯 TS + 手写 DOM | 可用性矩阵会散落 |
| React / Svelte | 生态不占优、无收益 |
| 组件库 | 依赖体积与升级噪音，收益不足 |
| Pinia | 状态总量太小 |
| Vue Router | 单页 |

## 后续

1. 是否引入 `ts-rs` / `specta` 自动生成前端类型：**若类型对拍的 CI 步骤开始反复出问题，就引入**；
2. 如果 GUI 之后要长出「设置页 / 账号页」（比如把扩展的部分配置搬过来），
   再评估组件库与路由 —— 那时状态量会变，本 ADR 的前提也就变了；
3. 国际化：首版只做中文；`i18n` 留到有第二个语言需求时再加（文案集中在 `src/locales/zh-CN.ts` 一个文件里，便于将来切换）。
