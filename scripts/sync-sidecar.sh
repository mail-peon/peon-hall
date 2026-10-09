#!/usr/bin/env bash
# 开发用：把**本地姊妹仓库**编译出来的中继二进制放到 Tauri 需要的位置（Linux/macOS）。
#
# 用法：scripts/sync-sidecar.sh [debug|release] [core-repo]
set -euo pipefail

configuration="${1:-debug}"
core_repo="${2:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/peon-burrow}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

triple="$(rustc --print host-tuple)"
source_bin="$core_repo/target/$configuration/burrow"
[ -f "$source_bin" ] || {
  echo "找不到 $source_bin —— 先在 core 仓库里跑：cargo build -p peon-burrow --$configuration" >&2
  exit 1
}

# ⓪ 新鲜度闸：源码比二进制新 = 忘了重新构建（`cargo test` 不会重建 bin）
newest_source=$(find "$core_repo/crates" "$core_repo/Cargo.toml" -name '*.rs' -o -name 'Cargo.toml' 2>/dev/null \
    | xargs ls -t 2>/dev/null | head -1)
if [ -n "$newest_source" ] && [ "$newest_source" -nt "$source" ]; then
    echo "错误：$source 比 $newest_source 旧 —— 先 cargo build -p peon-burrow" >&2
    exit 1
fi

# ① Tauri 打包用的位置（配置名 `binaries/burrow` + triple）
mkdir -p "$root/src-tauri/binaries"
cp "$source_bin" "$root/src-tauri/binaries/burrow-$triple"
echo "✅ $root/src-tauri/binaries/burrow-$triple"

# ② 运行时位置（tauri dev 时 current_exe 在 src-tauri/target/<profile>/）
runtime_dir="$root/src-tauri/target/$configuration"
if [ -d "$runtime_dir" ]; then
  cp "$source_bin" "$runtime_dir/burrow"
  echo "✅ $runtime_dir/burrow"
fi

echo
echo "这只是一个开发期的拷贝：发版时用的是从 core release 下载并**校验过 sha256** 的二进制。"
