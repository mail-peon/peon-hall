# ADR-1002 · 落地时与设计文档不一致的地方

- 状态：**已生效**（D0–D4 实现完成：前端、Rust 侧、CI）
- 日期：2026-10-09
- 影响文档：`00-overview.md`、`01-ui-and-states.md`、`02-core-integration.md`、`03-build-and-sidecar.md`、`04-release.md`

> 本仓库的局部 ADR 用 1000+ 段（见 [`adr-1001`](./adr-1001-frontend-stack.md)）。
> ⚠️ 这份 ADR **优先于**上面文档里被点名的段落。

---

## 1. `peon-burrow-ipc` 现在是 **path 依赖**（临时）

- 文档：钉 git tag（`{ git = "…", tag = "v0.1.0" }`），**本地联调才用 path 且不许提交**
- 实际：`src-tauri/Cargo.toml` 里就是 `path = "../../peon-burrow/crates/peon-burrow-ipc"`
- 理由：两个仓库都还在 0.0.0 占位阶段，core 还没发版；这期间用 path 才能「改 core 立刻生效」。
- ⚠️ **必须在首次发版前换掉**（换成一个 PR，见 `README.md` 的「本地联调」小节）。
  提交 path 依赖的后果是 CI 与别人的 checkout 都找不到那个路径。

## 2. capabilities **比文档更严**：没有 `shell:allow-execute`

- 文档（`03-build-and-sidecar.md § 4`）：给 sidecar 配 `shell:allow-execute` + 参数校验数组
- 实际：`capabilities/default.json` 只有 `core:default` 与 `clipboard-manager:allow-write-text`
- 理由：sidecar 由 **Rust 侧**用 `std::process::Command` 直接 spawn（为了拿到真实路径做提权、
  以及精确控制参数与输出解析）。前端因此**没有任何执行能力**，只能调本仓库那几个类型化命令
  （`snapshot` / `service_action` / `control_command` …），而 `control_command` 后面是
  `Request` 枚举白名单。这比「给前端一个受限的 shell」更小。
- `bundle.externalBin` 仍然保留（二进制要进安装包）—— 那是打包配置，不是权限。

## 3. NSIS 安装模式字段名：`currentUser`（不是 `perUser`）

- 文档：`nsis.installMode: "perUser"`
- 实际：`"currentUser"`（Tauri 2.12 的 schema 只认 `currentUser | perMachine | both`）
- 语义不变：**只给当前用户装**，避免应用继承提权状态（tauri-apps/tauri#9835）。
- 代价：文档里的字段名是错的，会让 `cargo build` 直接失败（我踩了）。

## 4. core 的 CLI 多了两处（为了让文档里的调用形式成立）

- 文档假设 `burrow service install --mode <user|system>` 与 `burrow service autostart on|off`
- 实际：core 原来只有 `--system`，也没有 `autostart` 子命令 → **在 core 侧补齐了**
  （`--system` 与 `--mode` 互斥，`--mode` 不认识的值报错并列出可选值）。
- 这条记在这里，因为「桌面端调了什么」与「core 提供什么」是两个仓库之间的契约。

## 5. 发现文件的字段：`{ kind, address, token, pid }`

- 文档：`{ schema, token, kind, path, pid }`
- 实际：以 `peon-burrow-ipc::ControlEndpoint` 为准 —— `kind` / `address` / `token`，
  外加**可选的** `pid`（core 在 `burrow run` 时写入自己的 pid）。
- 没有 `schema` 字段：协议版本在每一帧里（`v`），发现文件本身不需要再带一个。
- Windows 上路径是 `%LOCALAPPDATA%\peon-burrow\data\control.json`（**多一层 `data\`**，
  见 `02-core-integration.md § 2.1` 的说明）。

## 6. 五个按钮的可用性：以 `01-ui-and-states.md § 3` 的「何时可用」列为准

- 文档里有两处描述可用性：§ 2 的「主按钮 / 次按钮」与 § 3 的「何时可用」。
- 实际：§ 2 决定**哪个在前、哪个醒目**；§ 3 决定**能不能点**（并且禁用时必须给出原因）。
  两者的差别在「正在运行」时最明显：`卸载服务` 在 § 2 里不是主/次按钮，但 § 3 允许点。
- 卸载永远是 `btn-danger`（破坏性操作不该因为它是「次按钮」就变柔和）。

## 7. 诊断视图里没有重复的「扩展地址」行

- 文档 § 6 的示意里，诊断面板底部有一行 `扩展地址 + 【复制】`。
- 实际：主页面已经有一整行（`RelayAddress` 组件），诊断面板里不再重复。
  复制全部只复制检查项本身。
- 「建议动作」显示为 `建议：<core 给的文案>` + 一个【打开配置】按钮：core 的
  `doctor` 目前给的是**文案**（`CheckResult.action`），不是可执行的动作 id，
  所以桌面端不能凭它执行动作 —— 要执行得先让 core 把动作结构化。

## 8. 没有单独的【刷新状态】按钮

- 文档 § 3.1/§ 4 提到一个「刷新状态」的入口。
- 实际：刷新发生在挂载、窗口获得焦点、重新可见、以及**每次操作之后**；只有超时提示里
  才出现一个【刷新状态】按钮。单独放一个按钮的价值不如这几个自动时机。

## 9. 一条额外的黄色提示（文档没写，但属于 § 5 的边界）

服务管理器说「运行中」而控制面连不上时，显示 `连不上中继（可能正在重启或更新中）`。
**只在服务管理器说 running 时**才显示：仅仅「发现文件是旧的」不算（正常停掉的服务也会留下它）。

## 10. 版本注入：`core-version.txt` 只用于「关于/界面显示」

`src-tauri/src/commands.rs` 用 `include_str!("../../core-version.txt")` 在编译期把它带进界面，
真正的二进制来源由 `scripts/fetch-sidecar.sh`（CI 下载 + sha256 校验）决定。
两处不一致时，**以安装包里的二进制为准** —— 所以 release 工作流里那一步校验不能省。

---

## 仍然缓做的

- 真·提权辅助可执行文件（`burrow-elevate`）：现在靠 `powershell Start-Process -Verb RunAs`
  调同一个二进制（core 仓库 `adr-0006` 后续 1）；
- `ts-rs` / `specta` 自动生成前端类型：现在靠 `scripts/check-types.mjs` 对拍字段名（够用）；
- 界面国际化：文案已集中在 `src/locales/zh-CN.ts`，加语言只加文件；
- 签名（Windows OV/EV、macOS notarize）：首版未签名，用户会看到 SmartScreen / Gatekeeper 提示。
