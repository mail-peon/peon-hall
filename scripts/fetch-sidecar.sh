#!/usr/bin/env bash
# CI/发版用：从 core 的 GitHub Release 取中继二进制，**校验 sha256**，放到 Tauri 要的位置。
#
# 用法：scripts/fetch-sidecar.sh [core-ref] [target-triple]
#   core-ref      默认读仓库根 `bundle.toml` 的 `[core] version`（例：v0.1.0）
#   target-triple 默认 rustc --print host-tuple
#
# ⚠️ 必须校验：安装包里的核心二进制如果被换掉，等于给用户装了一个能看到邮箱凭据的后门。
# 这一步是供应链上最便宜的一道闸（ai-docs/03-build-and-sidecar.md § 2.1）。
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
config="$root/bundle.toml"

# 从 bundle.toml 读一个**扁平**的值（见那个文件顶部的说明；与 ci.yaml 的守卫同一套规则）。
#   read_config version  ->  [core] version 的值，去掉引号与行尾注释
read_config() {
  grep -E "^[[:space:]]*$1[[:space:]]*=" "$config" | head -1 \
    | sed -E 's/^[^=]*=[[:space:]]*"([^"]*)".*$/\1/'
}

core_ref="${1:-$(read_config version)}"
triple="${2:-$(rustc --print host-tuple)}"
repo="${CORE_REPO:-$(read_config repo)}"

[ -n "$core_ref" ] || { echo "bundle.toml 里读不到 core.version" >&2; exit 1; }
[ -n "$repo" ] || { echo "bundle.toml 里读不到 core.repo" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "取 $repo 的 $core_ref（$triple）"
# ⚠️ 优先 curl：core 是公开仓库，https 直连**不需要任何 token**（少一个变数），
#    而且这条路是本地逐 target 实测跑通的。`gh` 只作兜底 —— 它要 GH_TOKEN，
#    而 CI 里那个 token 属于本仓库，去读另一个仓库的资源可能被当成 404。
if command -v curl > /dev/null 2>&1; then
  base="https://github.com/$repo/releases/download/$core_ref"
  echo "  方式=curl  base=$base"
  curl -fsSL "$base/SHA256SUMS" -o "$work/SHA256SUMS" 2>/dev/null || true
  # 归档后缀按平台不同：Windows 是 `.zip`，其余是 `.tar.gz`（见 core 的两个 package-release 脚本）
  for suffix in zip tar.gz; do
    if curl -fsSL -o "$work/peon-burrow-$triple.$suffix" "$base/peon-burrow-$triple.$suffix"; then
      curl -fsSL -o "$work/peon-burrow-$triple.$suffix.sha256" \
        "$base/peon-burrow-$triple.$suffix.sha256" 2>/dev/null || true
      break
    fi
  done
elif command -v gh > /dev/null 2>&1; then
  echo "  方式=gh  仓库=$repo  pattern=peon-burrow-${triple}.*"
  # `peon-burrow-<triple>.*` 会同时带上归档与它自带的单文件 .sha256
  gh release download "$core_ref" --repo "$repo" \
    --pattern "peon-burrow-${triple}.*" --clobber --dir "$work"
  # 合并清单是**可选**的：core 的文档承诺了 `SHA256SUMS`，但到 v0.1.0 为止还没生成。
  gh release download "$core_ref" --repo "$repo" \
    --pattern 'SHA256SUMS' --clobber --dir "$work" 2>/dev/null || true
else
  echo "既没有 curl 也没有 gh，没法下载 core 二进制" >&2
  exit 1
fi

# 失败时先看这一行：下载到的到底是哪几个文件（空的话说明 release/tag/pattern 有一个不对）
echo "  已下载: $(ls -1 "$work" 2>/dev/null | tr '\n' ' ')"

# ⚠️ 校验和优先用合并清单，没有就退回归档自带的 `.sha256`。
#    这里**不能**硬依赖 `SHA256SUMS`：core 到 v0.1.0 为止都没生成它（它的 ai-docs 里
#    承诺了「一个 SHA256SUMS」，但 scripts/ 与 assets.yaml 里都没有那一步），
#    硬依赖会让第一个 bundle job 直接红在「没拿到 SHA256SUMS」。
if [ -f "$work/SHA256SUMS" ]; then
  # 只校验我们这个平台的产物（清单里还有别的平台）
  (cd "$work" && grep "peon-burrow-$triple" SHA256SUMS > mine.sums && sha256sum -c mine.sums)
else
  sum="$(ls "$work"/peon-burrow-"$triple".*.sha256 2>/dev/null | head -1 || true)"
  [ -n "$sum" ] || {
    echo "既没有 SHA256SUMS 也没有单文件校验和：$repo 的 $core_ref 资产不完整" >&2
    exit 1
  }
  (cd "$work" && sha256sum -c "$(basename "$sum")")
fi

archive="$(ls "$work"/peon-burrow-"$triple".* | grep -v sha256 | head -1)"
mkdir -p "$work/unpacked"
case "$archive" in
  *.zip) (cd "$work/unpacked" && unzip -q "$archive") ;;
  *.tar.gz) tar -xzf "$archive" -C "$work/unpacked" ;;
  *) echo "不认识的归档：$archive" >&2; exit 1 ;;
esac

binary="$work/unpacked/burrow"
[ -f "$binary" ] || { echo "归档里没有 burrow：$(ls "$work/unpacked")" >&2; exit 1; }

mkdir -p "$root/src-tauri/binaries"
suffix=""
case "$triple" in *windows*) suffix=".exe" ;; esac
cp "$binary" "$root/src-tauri/binaries/burrow-$triple$suffix"
echo "✅ src-tauri/binaries/burrow-$triple$suffix（来自 $core_ref，sha256 已校验）"
