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
if command -v gh > /dev/null 2>&1; then
  # `peon-burrow-<triple>.*` 会同时带上归档与它自带的单文件 .sha256
  gh release download "$core_ref" --repo "$repo" \
    --pattern "peon-burrow-${triple}.*" --clobber --dir "$work"
  # 合并清单是**可选**的：core 的文档承诺了 `SHA256SUMS`，但到 v0.1.0 为止还没生成。
  # 拿不到就退到单文件校验和 —— 两者信任级别一样（都来自同一个 release）。
  gh release download "$core_ref" --repo "$repo" \
    --pattern 'SHA256SUMS' --clobber --dir "$work" 2>/dev/null || true
else
  # 没有 gh 时退回 curl（公开仓库不需要 token）
  base="https://github.com/$repo/releases/download/$core_ref"
  curl -fsSL "$base/SHA256SUMS" -o "$work/SHA256SUMS" 2>/dev/null || true
  # 归档后缀按平台不同：Windows 是 `.zip`，其余是 `.tar.gz`（见 core 的两个 package-release 脚本）
  for suffix in zip tar.gz; do
    if curl -fsSL -o "$work/peon-burrow-$triple.$suffix" "$base/peon-burrow-$triple.$suffix"; then
      curl -fsSL -o "$work/peon-burrow-$triple.$suffix.sha256" \
        "$base/peon-burrow-$triple.$suffix.sha256" 2>/dev/null || true
      break
    fi
  done
fi

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
