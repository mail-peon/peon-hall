# 04 · 发版

> 规则与 peon-burrow 仓库对齐（tag、幂等、产物命名），但本仓库**没有**自更新清单要做。
> peon-burrow 侧规则：`ai-docs/05-release-and-versioning.md`、`ai-docs/decisions/adr-0007-release-pipeline.md`
> （本地 [`../../peon-burrow/ai-docs/05-release-and-versioning.md`](../../peon-burrow/ai-docs/05-release-and-versioning.md)）。

---

## 1. 版本号：只有一个来源

**`package.json` 的 `version` 是唯一来源。** 其余两处必须是它的副本：

| 位置 | 谁读 | 怎么保证一致 |
| --- | --- | --- |
| `package.json` | npm 生态、发版脚本 | **源头** |
| `src-tauri/tauri.conf.json` 的 `version` | Tauri 打进安装包元数据与文件名 | ① 本地：`pnpm version:bump` 脚本同时改两处；② CI：断言两者相等 |
| git tag | CI 触发 | CI：断言 `v${package.json.version} == tag` |

⚠️ 参考项目里踩过的两个极端：

- `w3wright-studio`：`tauri.conf.json` = `0.1.0`、`Cargo.toml` = `0.1.0`、`package.json` = `0.0.0`
  —— **四处各写各的**，没人知道该信谁；
- `clash-verge-rev`：CI 里加了一步「tag 必须等于 `package.json` 的 version」→ 至少不会发错版本号。

**照 `clash-verge-rev` 的做法**，并在 bump 脚本里一次改两处（不允许手改）。

> `src-tauri/Cargo.toml` 的 `version` 不参与安装包命名（Tauri 以 `tauri.conf.json` 为准），
> 但为了 `cargo metadata` 不困惑，也让 bump 脚本一并改掉。

---

## 2. 发版流程

```bash
# 1) 确认 core 已发版，并更新绑定的 core 版本
cat core-version.txt                  # 例：v0.1.0
#    如需升级： echo v0.2.0 > core-version.txt

# 2) 跑本地检查
pnpm install
pnpm typecheck && pnpm build          # 前端
cargo fmt --all --check --manifest-path src-tauri/Cargo.toml
cargo clippy --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings

# 3) 改版本（同时改 package.json 与 tauri.conf.json）
pnpm version:bump 0.1.1

# 4) 提交 + 打 tag + 推
git add -A && git commit -m "chore: release v0.1.1"
git tag v0.1.1 && git push origin main --tags

# 5) 等 CI
gh run watch
```

> 也可以继续用作者生态里的 JS `bumpp`（`pnpm release`），只要它同时改到
> `tauri.conf.json`（在 `bumpp` 的配置里加 `files`）。

---

## 3. CI（`.github/workflows/release.yaml`）

```yaml
name: Release

on:
  push:
    # 唯一触发点。与 peon-burrow 仓库一致：各自仓库各自用 v*.*.*（没有前缀）
    tags: [ "v*.*.*" ]

permissions:
  contents: read

jobs:
  checks:
    name: Checks
    uses: ./.github/workflows/ci.yaml     # 前端 typecheck/build + src-tauri fmt/clippy/test

  build:
    name: Bundle (${{ matrix.target }})
    needs: [ checks ]
    strategy:
      fail-fast: false
      matrix:
        include:
          - runner: windows-latest
            target: x86_64-pc-windows-msvc
          - runner: macos-15-intel
            target: x86_64-apple-darwin
          - runner: macos-15
            target: aarch64-apple-darwin
          - runner: ubuntu-latest
            target: x86_64-unknown-linux-gnu
    runs-on: ${{ matrix.runner }}
    permissions:
      contents: write
```

> ⚠️ **`ci.yaml` 没有自己的触发**（`on: workflow_call`）：本仓库唯一的自动化入口就是
> `release.yaml`，而且**只吃 `v*.*.*` tag**（没有 `push` / `pull_request` / `workflow_dispatch`）。
> 也就是说检查发生在发版那一刻。失败时**不要移动 tag** —— 从 Actions 页面重跑失败的 job 即可。

每个 matrix leg 的步骤：

| # | 步骤 | 关键点 |
| --- | --- | --- |
| 1 | `actions/checkout@v7` | |
| 2 | 校验 tag == `package.json` 的 version | 早失败，别等到打完包 |
| 3 | `dtolnay/rust-toolchain@master` with `targets: ${{ matrix.target }}` | Tauri 需要指定 target |
| 4 | `Swatinem/rust-cache@v2`（`workspaces: src-tauri -> target`） | |
| 5 | Linux 系统依赖（WebKitGTK 等） | 仅 ubuntu |
| 6 | `actions/setup-node@v7` + `pnpm/action-setup@v6` + `pnpm install --frozen-lockfile` | |
| 7 | **取 core 二进制**：`core_ref`（输入或 `core-version.txt`）→ `gh release download` → `sha256sum -c` → 重命名成 `<name>-<triple>` | 见 [`03-build-and-sidecar.md § 2.1`](./03-build-and-sidecar.md) |
| 8 | `tauri-apps/tauri-action@v1.0.0`，`args: --target ${{ matrix.target }}` | **不设** `includeUpdaterJson`、**不给** `TAURI_SIGNING_PRIVATE_KEY` |
| 9 | release notes 里写清「捆绑 core vX.Y.Z」 | 用 `--notes` 或先写文件再传给 action |

> ⚠️ `tauri-action` 的版本要**钉死**（`@v1.0.0`）。参考项目的注释里写着
> 「上游改了 `latest.json` 的生成逻辑，故锁定版本」—— 我们不用 updater，
> 但同样不希望构建行为随上游漂移。

---

## 4. 产物清单

| 平台 | 产物 |
| --- | --- |
| Windows x64 | `peon-hall_0.1.0_x64_en-US.msi`、`peon-hall_0.1.0_x64-setup.exe`（NSIS） |
| macOS x64 / arm64 | `peon-hall_0.1.0_x64.dmg` / `_aarch64.dmg`（+ `.app`） |
| Linux x64 | `peon-hall_0.1.0_amd64.AppImage`、`peon-hall_0.1.0_amd64.deb` |

`productName` 已定为 **`peon-hall`**（产品命名决策见 core 仓库 `ai-docs/decisions/adr-0001-two-repos.md`）：
两个选项：

| 项 | 结论 |
| --- | --- |
| 安装包文件名 | `peon-hall_0.1.0_x64-setup.exe`：无中文、无空格，链接不必编码、脚本不必加引号 |
| 窗口标题 / 系统里显示的名字 | 同样用 `peon-hall` |

⚠️ Tauri 会把 `productName` 直接放进文件名，所以**不要**在里面写中文或空格。
已同步到 [`03-build-and-sidecar.md § 3`](./03-build-and-sidecar.md) 的配置示例。

---

## 5. 发版前 checklist

| # | 检查 |
| --- | --- |
| 1 | `core-version.txt` 指向一个**已存在**的 core tag，且该 tag 的资产能被下载 |
| 2 | `package.json`、`tauri.conf.json`、`src-tauri/Cargo.toml` 版本一致 |
| 3 | 本地 `pnpm tauri build` 出的安装包里 sidecar 的 sha256 与 core release 一致 |
| 4 | 干净机器上装一次：主路径零 UAC；五种操作都对 |
| 5 | 界面里显示的 core 版本与 `core-version.txt` 一致 |
| 6 | 没有 updater 产物（release 里不应出现 `.sig` / `latest.json`） |

---

## 6. 出问题怎么办

| 情况 | 处置 |
| --- | --- |
| 某个平台构建失败 | 不要移动 tag；修好 → 从 Actions 重跑失败 job |
| sidecar 下载 404（core tag 不存在） | 确认 core 已发版；或把 `core_ref` 指到存在的 tag；重跑 |
| 安装包装完打不开 | 检查 sidecar 是否被正确 bundle（Windows 上是否被 SmartScreen 拦 —— 未签名时的正常现象，见下） |
| 发现严重 bug | 发补丁版本。**不要删 release**（用户下载链接会 404） |

---

## 7. 已知的未签名代价（首版）

| 平台 | 现象 | 缓解 |
| --- | --- | --- |
| Windows | SmartScreen：「Windows 已保护你的电脑」 | 文档里给出「更多信息 → 仍要运行」的截图指引；后续买 OV/EV 证书 |
| macOS | Gatekeeper：无法验证开发者 | 文档里给出「右键 → 打开」的指引；后续 Apple Developer 签名 + notarize |
| Linux | 无 | —— |

> 这与 core 的**更新通道签名**是两件事：前者解决「用户装的时候被拦」，
> 后者解决「更新时不被投毒」。peon-burrow 侧至少要做后者（peon-burrow 仓库 `adr-0005`）。

---

## 8. 验收

| # | 断言 |
| --- | --- |
| 1 | tag 与 `package.json` 不一致时 CI 直接失败（不出包） |
| 2 | 四个 target 都能出安装包（两个 macOS 架构分别构建） |
| 3 | release 里有 release notes 说明捆绑的 core 版本 |
| 4 | release 里没有 `.sig` / `latest.json` |
| 5 | 从 release 下载安装包 → 干净机器 → 完成 `00-overview.md § 6` 的七步验收 |
