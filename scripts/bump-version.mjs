#!/usr/bin/env node
// 版本号：**唯一来源是 package.json**，这个脚本一次改三处（不允许手改）。
//
// 用法：pnpm version:bump 0.1.1
//
// 为什么不手改：参考项目里出现过 tauri.conf.json = 0.1.0、Cargo.toml = 0.1.0、
// package.json = 0.0.0 —— 四处各写各的，没人知道该信谁（ai-docs/04-release.md § 1）。
import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'

const version = process.argv[2]
if (!version || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error('用法：pnpm version:bump <x.y.z>（例：pnpm version:bump 0.1.1）')
  process.exit(1)
}

const root = resolve(import.meta.dirname, '..')
const changed = []

// ① 源头：package.json
const pkgPath = resolve(root, 'package.json')
const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'))
pkg.version = version
writeFileSync(pkgPath, `${JSON.stringify(pkg, null, 2)}\n`)
changed.push('package.json')

// ② Tauri 拿它打安装包元数据与文件名
const confPath = resolve(root, 'src-tauri/tauri.conf.json')
const conf = JSON.parse(readFileSync(confPath, 'utf8'))
conf.version = version
writeFileSync(confPath, `${JSON.stringify(conf, null, 2)}\n`)
changed.push('src-tauri/tauri.conf.json')

// ③ src-tauri/Cargo.toml：不参与安装包命名，但让它与别处一致，免得 cargo metadata 让人困惑
const cargoPath = resolve(root, 'src-tauri/Cargo.toml')
const cargo = readFileSync(cargoPath, 'utf8')
const updated = cargo.replace(/^version = "[^"]+"/m, `version = "${version}"`)
if (updated === cargo) {
  console.error('⚠️ src-tauri/Cargo.toml 里没找到顶层 version = "…"')
  process.exit(1)
}
writeFileSync(cargoPath, updated)
changed.push('src-tauri/Cargo.toml')

console.log(`已统一改成 ${version}：`)
for (const file of changed) console.log(`  ${file}`)
console.log('下一步：git add -A && git commit -m "chore: release v' + version + '" && git tag v' + version)
