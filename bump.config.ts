import { defineConfig } from 'bumpp'

// 发版入口：`pnpm release`（就是 `bumpp`，默认交互式问你升哪个版本）。
//
// 版本号的**唯一来源是 `package.json`**，这里负责让另外两个文件跟着一起动
// （`.github/workflows/ci.yaml` 会断言三处一致，不一致就红）：
//   - `src-tauri/tauri.conf.json`：JSON，bumpp 自己解析
//   - `src-tauri/Cargo.toml`：TOML，bumpp 不认识它，走「全局文本替换」这条路
//     （实测这份 Cargo.toml 里旧版本号只出现在 `[package] version` 那一行，不会误伤依赖）
//
// bumpp 默认就 commit + tag + push（要停用任意一步加 `--no-push` 之类）——
// 推 tag 才会触发 `.github/workflows/release.yaml`，所以这里不要关掉它。
// 提交信息默认就是 Conventional Commits 的 `chore: release v{version}`，显式写出来只是为了防止上游改默认值。
export default defineConfig({
  files: ['package.json', 'src-tauri/tauri.conf.json', 'src-tauri/Cargo.toml'],
  // ⚠️ 反向命名、而且**默认是跳过检查**的（`noGitCheck` 默认 `true`）——所以显式设 false 才算打开。
  //    打开之后：工作区不干净就不发版。不然那笔无关改动可能被 bumpp 一起提交。
  noGitCheck: false,
  commit: 'chore: release v{version}',
  tag: 'v{version}',
})
