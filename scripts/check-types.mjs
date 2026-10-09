#!/usr/bin/env node
// 对拍：`src/types.ts` 里的字段名必须都能在 Rust 侧找到（ADR-1001 Q2 的 ⚠️）。
//
// 为什么需要它：前端手写 TS 接口（不引 ts-rs/specta），Rust 改了字段名时**前端不会编译报错**，
// 运行时的表现是「界面状态全是空的」—— 最难查的那种。这个脚本把漂移挡在 CI 上。
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

// 每个 TS interface 对应：哪个 crate 的哪个 Rust struct（或本仓库的哪个文件）
const mappings = [
  { ts: 'ServiceStatus', crate: 'peon-burrow-ipc-types', rust: 'ServiceStatus' },
  { ts: 'ProcessStatus', crate: 'peon-burrow-ipc-types', rust: 'ProcessStatus' },
  { ts: 'DoctorReport', crate: 'peon-burrow-ipc-types', rust: 'DoctorReport' },
  { ts: 'CheckResult', crate: 'peon-burrow-ipc-types', rust: 'CheckResult' },
  { ts: 'Snapshot', crate: 'peon-hall', rust: 'Snapshot', file: 'src-tauri/src/snapshot.rs' },
  { ts: 'Heartbeat', crate: 'peon-hall', rust: 'Heartbeat', file: 'src-tauri/src/commands.rs' },
  { ts: 'ActionResult', crate: 'peon-hall', rust: 'ActionResult', file: 'src-tauri/src/commands.rs' },
  { ts: 'AppInfo', crate: 'peon-hall', rust: 'AppInfo', file: 'src-tauri/src/commands.rs' },
]

const tsSource = readFileSync(typesPath, 'utf8')

/** 取一个 TS interface 的字段名（只取顶层，忽略注释与嵌套）。 */
function tsFields(name) {
  const start = tsSource.indexOf(`export interface ${name} {`)
  if (start < 0) throw new Error(`src/types.ts 里找不到 interface ${name}`)
  const body = tsSource.slice(tsSource.indexOf('{', start) + 1)
  const end = body.indexOf('\n}')
  const lines = body.slice(0, end).split('\n')
  const fields = []
  for (const line of lines) {
    const text = line.trim()
    if (text.startsWith('//') || text.startsWith('*') || text.startsWith('/*') || text === '') continue
    const match = /^([A-Za-z_][A-Za-z0-9_]*)\??:/.exec(text)
    if (match) fields.push(match[1])
  }
  return fields
}

/** camelCase → snake_case（Rust 侧的 serde(rename_all = "camelCase") 就是它的逆）。 */
function toSnake(name) {
  return name.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase()
}

/** 取一个 Rust struct 的字段名。 */
function rustFields(source, name) {
  const start = source.indexOf(`pub struct ${name} {`)
  if (start < 0) throw new Error(`找不到 pub struct ${name}`)
  const body = source.slice(source.indexOf('{', start) + 1)
  const end = body.indexOf('\n}')
  const fields = []
  for (const line of body.slice(0, end).split('\n')) {
    const match = /^\s*pub ([a-z_][a-z0-9_]*):/.exec(line)
    if (match) fields.push(match[1])
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

  for (const field of actual) {
    const snake = toSnake(field)
    if (!expected.includes(snake)) {
      problems.push(
        `${mapping.ts}.${field} 在 Rust 的 ${mapping.rust} 里找不到（期望字段 ${snake}）—— ${file}`,
      )
    }
  }

  console.log(`✅ ${mapping.ts} ← ${mapping.crate}::${mapping.rust}（${actual.length} 个字段）`)
}

if (problems.length > 0) {
  console.error('\n❌ 前端类型与 Rust 侧不一致：')
  for (const problem of problems) console.error(`   ${problem}`)
  process.exit(1)
}
console.log('\n前端类型与 Rust 侧一致')
