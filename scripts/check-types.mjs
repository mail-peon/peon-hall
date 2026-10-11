#!/usr/bin/env node
// 对拍：`src/types.ts` 与 Rust 侧的**字段名 + 可空性**必须双向一致（ADR-1001 Q2 的 ⚠️）。
//
// 为什么需要它：前端手写 TS 接口（不引 ts-rs/specta），Rust 改了字段名时**前端不会编译报错**，
// 运行时的表现是「界面状态全是空的」—— 最难查的那种。这个脚本把漂移挡在 CI 上。
//
// 三个方向都查（从前只查了第一个）：
//   ① TS 有的字段，Rust 必须有（不然前端读到 undefined）；
//   ② Rust 有的字段，TS 必须有（不然界面少一块信息，而没人会发现）；
//   ③ 可空性一致：TS 的 `?` / `| null` 必须对 Rust 的 `Option<...>`，反之亦然
//      （`Snapshot.service` 这种 `Option` 字段就是靠这条兜住的 —— 只对字段名的
//      脚本对「Rust 变 Option 而 TS 还是非空」完全不报警）。
//
// 做法：用 `cargo metadata` 找到定义这些类型的 crate 的**真实源码路径**
// （path 依赖就是本地路径，git 依赖在 ~/.cargo/git/checkouts，registry 依赖在 ~/.cargo/registry），
// 然后把 TS 的 camelCase 字段名换成 snake_case 去源码里找。
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const typesPath = resolve(root, 'src/types.ts')

// 每个 TS interface 对应：哪个 crate 的哪个 Rust struct（或本仓库的哪个文件）。
//
// `ignoreRustFields`：Rust 侧有、前端**故意**不用的字段（必须写原因，否则下一个人
// 会以为这里是漏掉的）。
const mappings = [
  { ts: 'ServiceStatus', crate: 'peon-burrow-ipc-types', rust: 'ServiceStatus' },
  { ts: 'ProcessStatus', crate: 'peon-burrow-ipc-types', rust: 'ProcessStatus' },
  { ts: 'DoctorReport', crate: 'peon-burrow-ipc-types', rust: 'DoctorReport' },
  { ts: 'CheckResult', crate: 'peon-burrow-ipc-types', rust: 'CheckResult' },
  { ts: 'Snapshot', crate: 'peon-hall', rust: 'Snapshot', file: 'src-tauri/src/snapshot.rs' },
  { ts: 'Heartbeat', crate: 'peon-hall', rust: 'Heartbeat', file: 'src-tauri/src/commands.rs' },
  { ts: 'ActionResult', crate: 'peon-hall', rust: 'ActionResult', file: 'src-tauri/src/commands.rs' },
  { ts: 'AppInfo', crate: 'peon-hall', rust: 'AppInfo', file: 'src-tauri/src/commands.rs' },
  // 下面两个从前没在对拍里：`CommandError` 的 `suggestDoctor` 与 `CommandEvent` 的四个字段
  // 一旦漂移，抽屉里的报错提示会静默失效（H-5）。
  { ts: 'CommandError', crate: 'peon-hall', rust: 'CommandError', file: 'src-tauri/src/commands.rs' },
  { ts: 'CommandEvent', crate: 'peon-hall', rust: 'CommandEvent', file: 'src-tauri/src/commands.rs' },
]

const tsSource = readFileSync(typesPath, 'utf8')

/**
 * 取一个 TS interface 的字段（只取顶层，忽略注释）：
 * 名字、是不是可选（`?`）、类型文本（跨行的联合类型会拼起来）。
 */
function tsFields(name) {
  const start = tsSource.indexOf(`export interface ${name} {`)
  if (start < 0) throw new Error(`src/types.ts 里找不到 interface ${name}`)
  const body = tsSource.slice(tsSource.indexOf('{', start) + 1)
  const end = body.indexOf('\n}')
  const lines = body.slice(0, end).split('\n')
  const fields = []
  let current = null
  for (const line of lines) {
    const text = line.trim()
    if (text.startsWith('//') || text.startsWith('*') || text.startsWith('/*') || text === '') continue
    const match = /^([A-Za-z_][A-Za-z0-9_]*)(\?)?:\s*(.*)$/.exec(text)
    if (match) {
      current = { name: match[1], optional: match[2] === '?', type: match[3].trim() }
      fields.push(current)
    } else if (current) {
      // 类型跨行（`a:\n  | null`）：接着上一行
      current.type = `${current.type} ${text}`.trim()
    }
  }
  return fields
}

/** camelCase → snake_case（Rust 侧的 serde(rename_all = "camelCase") 就是它的逆）。 */
function toSnake(name) {
  return name.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase()
}

/** snake_case → camelCase（反向检查用）。 */
function toCamel(name) {
  return name.replace(/_([a-z0-9])/g, (_, char) => char.toUpperCase())
}

/** 取一个 Rust struct 的字段：名字 + 是不是 `Option<...>`（后者用来对拍可空性）。 */
function rustFields(source, name) {
  const start = source.indexOf(`pub struct ${name} {`)
  if (start < 0) throw new Error(`找不到 pub struct ${name}`)
  const body = source.slice(source.indexOf('{', start) + 1)
  const end = body.indexOf('\n}')
  const fields = []
  for (const line of body.slice(0, end).split('\n')) {
    const match = /^\s*pub ([a-z_][a-z0-9_]*):\s*(.+?),?\s*$/.exec(line)
    if (match) {
      fields.push({ name: match[1], optional: match[2].trim().startsWith('Option<') })
    }
  }
  return fields
}

// 找到每个 crate 的源码目录
const metadata = JSON.parse(
  execFileSync('cargo', ['metadata', '--manifest-path', 'src-tauri/Cargo.toml', '--format-version', '1'], {
    cwd: root,
    maxBuffer: 64 * 1024 * 1024,
    encoding: 'utf8',
  }),
)

const packages = new Map(metadata.packages.map((pkg) => [pkg.name, pkg]))
const problems = []
let checkedFields = 0

for (const mapping of mappings) {
  const pkg = packages.get(mapping.crate)
  if (!pkg) {
    problems.push(`cargo metadata 里没有 ${mapping.crate}（依赖清单变了？）`)
    continue
  }
  const file = mapping.file
    ? resolve(root, mapping.file)
    : resolve(dirname(pkg.manifest_path), 'src/lib.rs')
  const rust = readFileSync(file, 'utf8')
  const expected = rustFields(rust, mapping.rust)
  const actual = tsFields(mapping.ts)
  const ignore = mapping.ignoreRustFields ?? {}

  // ① TS → Rust（字段名），顺带 ③ 可空性
  for (const field of actual) {
    const snake = toSnake(field.name)
    const found = expected.find((entry) => entry.name === snake)
    if (!found) {
      problems.push(
        `${mapping.ts}.${field.name} 在 Rust 的 ${mapping.rust} 里找不到（期望字段 ${snake}）—— ${file}`,
      )
      continue
    }
    const tsNullable = field.optional || /\bnull\b/.test(field.type)
    if (tsNullable !== found.optional) {
      problems.push(
        `${mapping.ts}.${field.name} 的可空性不一致：TS ${tsNullable ? '可空' : '不可空'}，` +
          `Rust ${found.optional ? 'Option<...>' : '非 Option'}（${mapping.rust}.${found.name}）—— ${file}`,
      )
    }
    checkedFields += 1
  }

  // ② Rust → TS（漏字段）
  const tsNames = new Set(actual.map((field) => field.name))
  for (const entry of expected) {
    if (ignore[entry.name]) continue
    checkedFields += 1
    const camel = toCamel(entry.name)
    if (!tsNames.has(camel)) {
      problems.push(
        `${mapping.rust}.${entry.name} 在 ${mapping.ts} 里没有对应字段（期望 ${camel}）—— ${file}。` +
          '前端故意不用它的话，加到 mappings 的 ignoreRustFields 里并写原因',
      )
    }
  }

  const ignored = Object.keys(ignore).length
  console.log(
    `✅ ${mapping.ts} ← ${mapping.crate}::${mapping.rust}（${actual.length} 个字段` +
      `${ignored ? `，有意忽略 Rust 侧 ${ignored} 个` : ''}）`,
  )
}

if (problems.length > 0) {
  console.error('\n❌ 前端类型与 Rust 侧不一致：')
  for (const problem of problems) console.error(`   ${problem}`)
  process.exit(1)
}
console.log(`\n前端类型与 Rust 侧一致（字段名 + 可空性，共 ${checkedFields} 项）`)
