# 03 · 构建与 sidecar 打包

> 目标：**安装包里带着一个确定的 core 二进制**，用户不需要另外下载中继。
> 打包契约的决策依据在 peon-burrow 仓库 `ai-docs/decisions/adr-0006-desktop-installer.md § Q4`。

---

## 1. 版本绑定

| 文件 | 内容 | 谁维护 |
| --- | --- | --- |
| `core-version.txt`（仓库根） | 一个 tag，例 `v0.1.0` | 人工（或脚本）在 core 发版后更新；**进 git** |
| CI 输入 `core_ref` | 覆盖上面的 tag（默认读文件） | 发版时可选 |

发版产物里要能回答「这个安装包带的是哪个 core」→ release notes 里写，
并且 sidecar 的版本在界面「关于」里可见（读控制面 `version`）。

---

> ℹ️ 本文（以及本仓库其它文档）里的 **core** 都指姊妹仓库 **`peon-burrow`**（二进制 `burrow`）。
> `core-version.txt` / `core_ref` 这类名字保留，表示「绑定的 peon-burrow 版本」。

## 2. sidecar 命名契约（Tauri 要求）

```
src-tauri/binaries/
├── burrow-x86_64-pc-windows-msvc.exe
├── burrow-aarch64-apple-darwin
└── burrow-x86_64-unknown-linux-gnu
```

```jsonc
// src-tauri/tauri.conf.json
"bundle": {
  "externalBin": ["binaries/burrow"]
}
```

| 规则 | 说明 |
| --- | --- |
| 文件名 = 配置里的名字 + `-` + **target triple** | 不这么写，`tauri build` 直接报找不到文件 |
| triple 从哪来 | `rustc --print host-tuple`（Rust ≥ 1.84）；不要手写猜 |
| Windows 带 `.exe` 后缀 | 其余平台不带 |
| 运行时调用只用 **basename** | Rust：`app.shell().sidecar("burrow")` —— **不是**配置里的 `binaries/…` 路径 |
| 不与发布归档混淆 | 归档（`peon-burrow-<triple>.zip`）里是 `burrow[.exe]`（无 triple）；这里要重命名 |

**四个名字不要混**（最容易错的地方）：

| 名字 | 值 | 用在哪 |
| --- | --- | --- |
| 仓库 / crate | `peon-burrow` | GitHub 仓库名、`Cargo.toml` 包名、**发布归档**名前缀 |
| 归档里的二进制 | `burrow[.exe]` | 用户敲的命令、服务里跑的进程 |
| 本仓库的 sidecar 文件 | `burrow-<triple>[.exe]` | Tauri `externalBin` 要求「配置名 + target triple」 |
| 提权辅助（形态待定） | `burrow-elevate` | 见 peon-burrow 仓库 `adr-0006` 后续 1 |

所以链路是：从 release 下载 `peon-burrow-<triple>.zip` → 解压出 `burrow[.exe]`
→ 复制成 `src-tauri/binaries/burrow-<triple>[.exe]` → Tauri 打包时按 `externalBin` 收进去。

### 2.1 取二进制（构建前）

```bash
CORE_REF="$(cat core-version.txt)"
TRIPLE="$(rustc --print host-tuple)"
mkdir -p src-tauri/binaries

# 公开仓库，gh 不需要 token
gh release download "$CORE_REF" \
  --repo mail-peon/peon-burrow \
  --pattern "peon-burrow-${TRIPLE}.*" --clobber --dir /tmp/burrow-asset

# 校验（core 的 release 里带 SHA256SUMS）
gh release download "$CORE_REF" --repo mail-peon/peon-burrow \
  --pattern 'SHA256SUMS' --clobber --dir /tmp/burrow-asset
(cd /tmp/burrow-asset && sha256sum -c --ignore-missing SHA256SUMS)

# 解压 → 重命名成 Tauri 要的名字
#   .zip（Windows）/ .tar.gz（Unix）里是 burrow[.exe] + LICENSE
```

⚠️ **必须校验**：安装包里的核心二进制如果被换掉，等于给用户装了一个
能看到邮箱凭据的后门。这一步是供应链上最便宜的一道闸。

---

## 3. `tauri.conf.json` 关键字段

```jsonc
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "peon-hall",
  "version": "0.1.0",                        // ⚠️ 必须与 package.json 相同，见 04-release.md § 1
  "identifier": "cn.imba97.peon.hall",  // 反 DNS；**不要用下划线**
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [{
      "label": "main",
      "title": "peon-hall",
      "width": 560, "height": 460,
      "minWidth": 520, "minHeight": 420,
      "resizable": true
    }],
    "security": { "csp": "default-src 'self'; style-src 'self' 'unsafe-inline'" }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis", "msi", "app", "dmg", "appimage", "deb"],
    "createUpdaterArtifacts": false,          // ✅ 桌面端不做自更新
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"],
    "externalBin": ["binaries/burrow"]
  }
  // ❌ 没有 "plugins": { "updater": … } —— 需求明确不要
}
```

| 字段 | 取值理由 |
| --- | --- |
| `identifier` | 反 DNS、无下划线（macOS 包会挑刺）；与 core 的管道名/服务名无关 |
| `targets` | 三平台全要（需求 5/需求 4：三个桌面平台都发版）；Windows 同时出 `msi`（企业友好）与 `nsis`（体积小） |
| `createUpdaterArtifacts` | **false** —— 不生成 `.sig` 与 `latest.json`，也不需要签名私钥 |
| `csp` | 收紧到 `'self'`；只有样式需要 `unsafe-inline`（前端框架注入）。不要 `null` |

### 3.1 平台覆盖文件

```
src-tauri/tauri.windows.conf.json
src-tauri/tauri.macos.conf.json
src-tauri/tauri.linux.conf.json
```

| 平台 | 覆盖内容 |
| --- | --- |
| Windows | `nsis.installMode: "perUser"`（与「用户级自启」一致，**不是** `perMachine` —— 后者会让应用以提权状态启动，见下）；`webviewInstallMode: { type: "downloadBootstrapper" }` |
| macOS | `minimumSystemVersion: "11.0"`；`signingIdentity: null`（未签名，首版）；dmg 布局 |
| Linux | `deb.depends`；AppImage 的 category |

> ⚠️ 已知坑：从 `perMachine` / `both` 安装的 NSIS 包启动的应用**可能继承提权状态**
> （[tauri-apps/tauri#9835](https://github.com/tauri-apps/tauri/issues/9835)）。
> 我们的默认是 `perUser`；若将来要 `perMachine`，必须重新审视「非提权 GUI 走控制面」这条设计。

---

## 4. capabilities（Tauri 2 权限）

```jsonc
// src-tauri/capabilities/default.json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "状态卡片与 5 个操作所需的权限",
  "windows": ["*"],                          // ⚠️ 不能用 ["main"]（会解析成空集合，所有命令被拒）
  "permissions": [
    "core:default",
    {
      "identifier": "shell:allow-execute",
      "allow": [{
        "name": "binaries/burrow",  // 配置里的路径形式
        "sidecar": true,
        "args": [                            // ⚠️ 带参数时必须给校验数组，否则拒绝执行
          { "validator": "^service$" },
          { "validator": "^(install|uninstall|start|stop|status|autostart)$" },
          { "validator": "^--(mode|autostart|json)$" },
          { "validator": "^(user|system|logon|boot|off)$" }
        ]
      }]
    },
    "clipboard-manager:allow-write-text"
  ]
}
```

| 坑 | 说明 |
| --- | --- |
| `windows: ["main"]` | 解析成**空窗口集合** → 所有命令被拒（参考项目 `w3wright-studio` 的 README 记过这个坑） |
| sidecar 名字 | 用**配置里的路径形式**（`binaries/burrow`），不是 basename |
| `args` 校验数组 | 不写就不允许传参；写了就要覆盖所有用例（提权调用传的是同一批参数） |
| 提权调用 | 走 `shell` 的 execute 时，UAC 由**系统的 manifest** 决定；如果辅助可执行文件是另一个 sidecar，需要**单独**为它加一条 `allow` |

---

## 5. 三平台构建

| 平台 | runner | 备注 |
| --- | --- | --- |
| Windows | `windows-latest` | WebView2 用 bootstrapper（不预装时自动下载） |
| macOS | `macos-15`(arm64) / `macos-15-intel`(x64) | **分架构构建**（与 core 一致，不做 universal） |
| Linux | `ubuntu-latest` | 需要系统依赖：`libwebkit2gtk-4.1-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`、`patchelf`、`libxslt1.1` |

```yaml
# 参考项目 clash-verge-rev 的写法（我们沿用其结构，去掉 updater 部分）
- name: Install dependencies (ubuntu only)
  if: runner.os == 'Linux'
  run: |
    sudo apt-get update
    sudo apt-get install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
                            librsvg2-dev patchelf libxslt1.1
```

`tauri-action` 的 `with:` 用法（去掉 updater 相关的 env）：

```yaml
- uses: tauri-apps/tauri-action@v1.0.0
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  with:
    tagName: ${{ github.ref_name }}
    releaseName: ${{ github.ref_name }}
    releaseDraft: false
    tauriScript: pnpm
    args: --target ${{ matrix.target }}
```

> ⚠️ **不设** `includeUpdaterJson`，**不给** `TAURI_SIGNING_PRIVATE_KEY`
> —— 桌面端不做自更新，这两个东西只会制造「为什么没有 latest.json」的困惑。

---

## 6. 本地构建检查清单

| # | 检查 |
| --- | --- |
| 1 | `src-tauri/binaries/burrow-<triple>[.exe]` 存在且来自 release（sha256 校验过） |
| 2 | `pnpm tauri build` 后，安装目录里能查到 sidecar（Windows：安装位置下的 `burrow.exe`） |
| 3 | 启动 GUI 后，`service status` 能跑通（说明 sidecar 可执行、capabilities 配对了） |
| 4 | 断网后打开 GUI 仍能工作（**不要**在启动时联网检查任何东西） |
| 5 | 把 sidecar 删掉 → GUI 显示「安装不完整」而不是崩溃 |

---

## 7. 验收

| # | 断言 |
| --- | --- |
| 1 | 三平台 `pnpm tauri build` 都能产出安装包，且安装包里含 sidecar |
| 2 | sidecar 的 sha256 与 core release 的 `SHA256SUMS` 一致（CI 里断言） |
| 3 | 安装包的元数据里版本 == `package.json` 的版本 |
| 4 | 安装后 off-line 可用（无任何联网检查） |
| 5 | 安装包不包含 `latest.json` / `.sig` 之类的更新产物 |
| 6 | capabilities 的最小权限原则：只有 sidecar 执行 + 剪贴板写入，没有通配 shell |
| 7 | Windows 安装模式为 `perUser`（不要让应用继承提权） |
