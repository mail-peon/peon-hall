#!/usr/bin/env bash
# CI/发版用：从 core 的 GitHub Release 取中继二进制，**校验 sha256**，放到 Tauri 要的位置。
#
# 用法：scripts/fetch-sidecar.sh [core-ref] [target-triple]
#   core-ref      默认读仓库根的 core-version.txt（例：v0.1.0）
#   target-triple 默认 rustc --print host-tuple
#
# ⚠️ 必须校验：安装包里的核心二进制如果被换掉，等于给用户装了一个能看到邮箱凭据的后门。
# 这一步是供应链上最便宜的一道闸（ai-docs/03-build-and-sidecar.md § 2.1）。
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
core_ref="${1:-$(tr -d '[:space:]' < "$root/core-version.txt")}"
triple="${2:-$(rustc --print host-tuple)}"
repo="${CORE_REPO:-mail-peon/peon-burrow}"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "取 $repo 的 $core_ref（$triple）"
if command -v gh > /dev/null 2>&1; then
  gh release download "$core_ref" --repo "$repo" \
    --pattern "peon-burrow-${triple}.*" --pattern 'SHA256SUMS' --clobber --dir "$work"
else
  # 没有 gh 时退回 curl（公开仓库不需要 token）
  base="https://github.com/$repo/releases/download/$core_ref"
  curl -fsSL "$base/SHA256SUMS" -o "$work/SHA256SUMS"
  for suffix in zip tar.gz; do
    if curl -fsSL -o "$work/peon-burrow-$triple.$suffix" "$base/peon-burrow-$triple.$suffix"; then
      break
    fi
  done
fi

[ -f "$work/SHA256SUMS" ] || { echo "没拿到 SHA256SUMS" >&2; exit 1; }

# 只校验我们这个平台的产物（校验和清单里还有别的平台）
(cd "$work" && grep "peon-burrow-$triple" SHA256SUMS > mine.sums && sha256sum -c mine.sums)

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
