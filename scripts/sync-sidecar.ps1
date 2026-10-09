# 开发用：把**本地姊妹仓库**编译出来的中继二进制放到 Tauri 需要的位置。
#
# 用法：pwsh scripts/sync-sidecar.ps1 [-Configuration debug|release] [-CoreRepo <路径>]
#
# 命名契约（Tauri 要求「配置里的名字 + - + target triple」）：
#   src-tauri/binaries/burrow-x86_64-pc-windows-msvc.exe
# 见 ai-docs/03-build-and-sidecar.md § 2。
[CmdletBinding()]
param(
    [ValidateSet('debug', 'release')][string]$Configuration = 'debug',
    [string]$CoreRepo = (Join-Path (Split-Path -Parent $PSScriptRoot) '..\peon-burrow')
)

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$coreRepo = (Resolve-Path $CoreRepo).Path
$triple = (& rustc --print host-tuple).Trim()
if (-not $triple) { throw '拿不到 host triple（rustc 不在 PATH？）' }

$ext = if ($triple -like '*windows*') { '.exe' } else { '' }
$source = Join-Path $coreRepo "target\$Configuration\burrow$ext"
if (-not (Test-Path $source)) {
    throw "找不到 $source —— 先在 core 仓库里跑：cargo build -p peon-burrow --$Configuration"
}

# ① Tauri 打包用的位置（配置名 `binaries/burrow` + triple）
$binaries = Join-Path $root 'src-tauri\binaries'
New-Item -ItemType Directory -Force $binaries | Out-Null
$packaged = Join-Path $binaries "burrow-$triple$ext"
Copy-Item $source $packaged -Force
Write-Host "✅ $packaged"

# ② 运行时位置：tauri dev 时 current_exe 在 src-tauri/target/<profile>/，
#    桌面端就是在那儿找 `burrow[.exe]`（见 src/sidecar.rs）
$runtimeDir = Join-Path $root "src-tauri\target\$Configuration"
if (Test-Path $runtimeDir) {
    $runtime = Join-Path $runtimeDir "burrow$ext"
    Copy-Item $source $runtime -Force
    Write-Host "✅ $runtime"
} else {
    Write-Host "（src-tauri\target\$Configuration 还不存在：先跑一次 pnpm tauri dev/build，再跑本脚本）"
}

Write-Host "`n这只是一个开发期的拷贝：发版时用的是从 core release 下载并**校验过 sha256** 的二进制。"
