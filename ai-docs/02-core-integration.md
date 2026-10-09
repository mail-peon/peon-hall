# 02 · 与 peon-burrow 的集成

> GUI **不含中继逻辑、不含服务注册逻辑**。它只做两件事：
> ① 通过控制面读状态 / 发命令；② 以提权方式调用中继二进制的 `service` 子命令。
>
> 控制面协议的唯一实现在 peon-burrow 仓库的 `crates/peon-burrow-ipc`，
> 详细规格：peon-burrow 仓库 `ai-docs/design/control-plane-ipc.md`
> （本地 [`../../peon-burrow/ai-docs/design/control-plane-ipc.md`](../../peon-burrow/ai-docs/design/control-plane-ipc.md)）。

---

## 1. 依赖方式：`peon-burrow-ipc`（发布后走 crates.io 版本依赖）

> 过渡期（首次发布前）用 git 依赖 + tag 钉版；**发布之后改成版本依赖**（`peon-burrow-ipc = "0.1"`），
> 不再需要锁 commit（[`../../peon-burrow/ai-docs/decisions/adr-0009-crates-io-publishing.md`](../../peon-burrow/ai-docs/decisions/adr-0009-crates-io-publishing.md) § 6）。

```toml
# peon-hall/src-tauri/Cargo.toml
[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "time"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tauri = { version = "2.12", features = [] }
tauri-plugin-shell = "2.4"
tauri-plugin-process = "2.4"

# 控制面协议的唯一实现。**钉 tag，不要跟 main**：
# ① 安装器与某个 peon-burrow 版本的协议必须匹配；② 构建可复现。
peon-burrow-ipc = { git = "https://github.com/mail-peon/peon-burrow", tag = "v0.1.0", package = "peon-burrow-ipc" }
```

**规则**：

| 规则 | 理由 |
| --- | --- |
| 必须提交 `Cargo.lock` | git 依赖要锁到具体 commit；tag 被强推过就更需要 |
| 本地联调可以临时改成 `path = "../../peon-burrow/crates/peon-burrow-ipc"`，**但不许提交** | 提交后 CI 与别人的 checkout 都会找不到路径（写进 README 的「本地联调」小节） |
| peon-burrow 发版后，升级 tag 要单独一个 PR | 让「GUI 升级了依赖」这件事在历史里可见 |
| 破坏性字段改动由 peon-burrow 侧 bump minor，本仓库跟进 | 见 peon-burrow 仓库 `ai-docs/decisions/adr-0001-two-repos.md` § 决策 3 |

⚠️ **不要**在本仓库手写一份「镜像类型」：漂移之后的表现是
「GUI 状态全是空的」，而且编译期不报错。

---

## 2. 控制面客户端

### 2.1 找到通道

```
读 <data-dir>/control.json  →  { kind, address, token, pid }
      Windows : %LOCALAPPDATA%\peon-burrow\data\control.json
      macOS   : ~/Library/Application Support/peon-burrow/control.json
      Linux   : ~/.local/share/peon-burrow/control.json
```

> ⚠️ Windows 上多一层 `data\`：core 的 `Paths::discover()` 用 `directories::ProjectDirs`，
> 它在 Windows 会给 `data_local_dir()` 追加 `data`、给 `config_dir()` 追加 `config`
> （macOS / Linux 没有这一层）。桌面端用同样的推导（`src-tauri/src/discovery.rs`），
> 不要硬编码路径。
>
> 字段名以实现为准：`{ kind, address, token, pid }`
> （`peon-burrow-ipc::ControlEndpoint`）。`pid` 是**可选**的 —— 老版本写的文件里没有它；
> 有它时桌面端可以区分「服务没在跑」与「发现文件是旧的」。

| 情况 | 界面 |
| --- | --- |
| 文件不存在 | 服务没在跑（或从没装过）→ 组合③/④ |
| 文件里的 `pid` 已不存在 | 视为「陈旧」→ 当作没在跑，但诊断里说明「发现文件是旧的」 |
| 文件存在但连不上 | 组合②「已安装，未运行」+ `（可能正在重启）` |

⚠️ **发现文件不是权威**，只是缓存。权威顺序：控制面 → 服务管理器 → 发现文件。

### 2.2 调用

一行 JSON 请求 → 一行 JSON 响应，超时 2 秒：

```rust
// peon-hall/src-tauri/src/control.rs（骨架）
pub async fn request(req: Request) -> Result<Response, ControlError> {
    let handle = discovery::load()?;                 // control.json
    let stream = transport::connect(&handle).await?; // 本地 socket；失败再试 loopback TCP
    let line = serde_json::to_vec(&Envelope::new(req, &handle.token))?;
    write_line(stream, &line).await?;
    let resp: Envelope<Response> = read_line(stream, MAX_LINE).await?;
    resp.into_result()
}
```

| 命令 | 界面用途 |
| --- | --- |
| `ping` | 每 2 秒的轻量存活探测 |
| `status` | 卡片主字段（端口、连接数、运行时长、lastError） |
| `version` | 卡片版本号 + 关于信息 |
| `doctor` | 诊断视图 |
| `stop` | 【停止】 |
| `restart` | 【重启】 |
| `updateCheck` / `updateApply` | 显示「有新版本」+【立即更新】（桌面端自身不更新，见下） |
| `traceOn` / `traceOff` | 诊断视图里的「记录明文日志 60 秒」（**必须二次确认**） |

超过 8 KiB 的响应/请求由中继侧直接断开 —— GUI 不该构造那么大的请求。

---

## 3. 服务状态的读法：走 sidecar，不走 API

| 读什么 | 怎么读 | 为什么不直接用平台 API |
| --- | --- | --- |
| 是否已注册 / 是否自启 / 安装级别 | spawn **非提权** sidecar：`burrow service status --json` | 平台 API 在 GUI 里要写三套（SCM / launchd / systemd），且逻辑与中继的 `peon-burrow-service` 必然漂移 |
| 是否在跑 / 端口 / 版本 | 控制面 | 这是唯一权威来源 |
| 是否需要提权 | sidecar 返回的 `requiresElevation` 字段 | 提权判定依赖安装级别，只有中继知道 |

```jsonc
// burrow service status --json
{
  "installed": true,
  "running": false,
  "level": "user",              // user | system
  "autostart": "logon",         // logon | boot | off
  "name": "peon-burrow",
  "binaryPath": "C:\\Users\\me\\AppData\\Local\\Programs\\peon-burrow\\burrow.exe",
  "requiresElevation": false,
  "restartPolicyConfigured": true,
  "lastExitCode": 4
}
```

> ⚠️ 这段 JSON 的形状由 **`peon-burrow-ipc-types::ServiceStatus`** 定义（唯一真相）；
> `status` 命令返回的是 `StatusReport { process, service }` —— 界面**不要自己拼字段**（丙2/丁4）。

⚠️ `restartPolicyConfigured` 一定要显示（诊断里）：它是「服务崩了能不能自己回来」的唯一信号，
而它**很容易在安装时漏配**（peon-burrow 仓库 `ai-docs/service-lifecycle.md § 3` 第 9 步）。

---

## 4. 提权路径

GUI 自己**不能**安装服务（`CreateServiceW` / `sc.exe` → access denied），
而 Tauri 的 `WindowsConfig` 没有 elevation 字段（那是 exe manifest 的事）。

| 平台 | 做法 | 用户看到 |
| --- | --- | --- |
| Windows | spawn 一个**带 `requireAdministrator` manifest 的辅助可执行文件**（中继二进制的第二份构建，或独立的 `burrow-elevate`）；UAC 由系统弹出 | 一次 UAC |
| macOS | `osascript -e 'do shell script "…" with administrator privileges'` | 系统密码框 |
| Linux | `pkexec <安装目录>/burrow service install …` | polkit 认证框（无 agent 时失败 → 提示用终端执行） |

调用形态（Rust 侧，示意）：

```rust
pub async fn service_action(action: ServiceAction, opts: &InstallOptions) -> Result<Status> {
    let exe = sidecar_path()?;                 // 安装目录里的中继二进制（burrow[.exe]）
    let args = action.to_args(opts);           // ["service", "install", "--mode", "system", ...]

    if opts.needs_elevation() {
        elevate::run(&exe, &args).await       // 平台分支：runas / osascript / pkexec
    } else {
        plain::run(&exe, &args).await
    }
}
```

**用户取消提权**（UAC 拒绝 / `osascript` 返回 `User canceled.`）→
返回一个**专门的错误类型** `ElevationCancelled`，界面显示「已取消」而不是「失败」。

> ⚠️ 首版默认走**用户级**（零 UAC）。系统服务是高级选项，放在折叠区。
> 理由：中继只监听 `127.0.0.1`，与登录绑定完全够用（peon-burrow 仓库 `adr-0003`）。

---

## 5. 桌面端**不**做自更新（但能触发中继更新）

| | 谁更新 |
| --- | --- |
| GUI 自己 | 用户重新下载安装包。**不引入 `tauri-plugin-updater`**，不配 `plugins.updater`，不生成 `latest.json` |
| peon-burrow（服务） | peon-burrow 自己（有独立的清单/校验/替换/重启机制，见 peon-burrow 仓库 `ai-docs/design/update-flow.md`） |

GUI 能做的是：显示中继版本 → 调 `updateCheck` → 若有新版，显示
「中继有新版本 v0.2.0」+【立即更新】→ 调 `updateApply` →
**界面会看到控制面断开**，此时必须显示

```
正在更新并重启中继…（扩展会在几秒内自动重连）
```

而不是「错误：连接失败」。更新完成后（轮询 `ping` 恢复）刷新状态与新版本号。

---

## 6. 本地联调（两个仓库并排时）

```bash
# 1) 先把中继跑起来（前台，方便看日志）
cd ../peon-burrow && cargo run -p peon-burrow -- run --foreground

# 2) 桌面端临时改成路径依赖（**不要提交**）
cd ../peon-hall
#   src-tauri/Cargo.toml:
#   peon-burrow-ipc = { path = "../../peon-burrow/crates/peon-burrow-ipc" }
pnpm tauri dev
```

⚠️ sidecar 在 dev 模式下也要存在：把 `../../peon-burrow/target/debug/burrow[.exe]`
复制成 `src-tauri/binaries/burrow-<triple>[.exe]`（命名规则见
[`03-build-and-sidecar.md § 2`](./03-build-and-sidecar.md)）。`src-tauri/binaries/` 已在 `.gitignore` 里。

---

## 7. 验收

| # | 断言 |
| --- | --- |
| 1 | `peon-burrow-ipc` 是唯一来源：`src-tauri` 里没有任何手写的控制面类型 |
| 2 | `control.json` 不存在 / 陈旧 / 损坏，三种情况界面都有正确文案 |
| 3 | 控制面连不上时，界面不把它渲染成「错误弹窗」，而是「未运行」 |
| 4 | sidecar 的 `service status --json` 解析失败（中继换了字段）→ 显示「版本不匹配，请更新桌面端」而不是空白 |
| 5 | 提权取消 → `ElevationCancelled` → 显示「已取消」，状态不变 |
| 6 | 用户级操作从不触发提权（Windows 上用 Process Monitor 确认没有 UAC 提示） |
| 7 | `updateApply` 触发后，界面进入「更新中」并在恢复后显示新版本号 |
| 8 | 构建产物里搜不到 updater 插件（依赖与配置都验一遍） |
| 9 | 本地联调（路径依赖）只在本地出现 —— CI 只认 git 依赖 |
