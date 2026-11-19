<#
.SYNOPSIS
    清理 Rust 构建产物（target/），释放磁盘空间。
.DESCRIPTION
    默认删除 target/ 下的全部内容：cargo 会按需重建，测试专用 build 目录
    （如 wave5b-*）同样是可丢弃的 scratch。使用 -KeepRelease 时保留
    target/release。

    删除前拒绝在构建进行中执行；对 reparse point（junction/symlink）只删除
    链接本身，不递归进入其目标，避免误删外部目录。脚本只允许清理仓库根下的
    target，解析后的父目录不是仓库根即拒绝执行。
.EXAMPLE
    pwsh -NoProfile -File scripts\clean-target.ps1 -DryRun
    pwsh -NoProfile -File scripts\clean-target.ps1
    pwsh -NoProfile -File scripts\clean-target.ps1 -KeepRelease
#>
[CmdletBinding()]
param(
    [switch]$KeepRelease,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).Path
$target = Join-Path $root 'target'

if (-not (Test-Path -LiteralPath $target)) {
    Write-Host 'No target directory found'
    return
}

$resolvedTarget = (Resolve-Path -LiteralPath $target).Path
if ((Split-Path -Parent $resolvedTarget) -ne $root) {
    throw "refusing to clean '$resolvedTarget': not the repository target"
}

function Get-TreeSize([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return 0 }
    return (Get-ChildItem -LiteralPath $Path -Recurse -File -Force -ErrorAction SilentlyContinue |
            Measure-Object -Property Length -Sum).Sum
}

$builds = @(Get-Process -Name cargo, rustc -ErrorAction SilentlyContinue)
if ($builds.Count -gt 0 -and -not $DryRun) {
    throw "refusing to clean while a build is running (cargo/rustc pid: $(($builds | ForEach-Object Id) -join ', '))"
}

$before = Get-TreeSize $resolvedTarget
Write-Host ("Target size before: {0:N2} GB" -f ($before / 1GB))

$removed = 0
foreach ($item in Get-ChildItem -LiteralPath $resolvedTarget -Force) {
    if ($KeepRelease -and $item.Name -eq 'release') {
        Write-Host 'kept: release'
        continue
    }
    $size = if ($item.PSIsContainer) { Get-TreeSize $item.FullName } else { $item.Length }
    if ($DryRun) {
        Write-Host ("[DRY-RUN] would remove: {0} ({1:N1} MB)" -f $item.Name, ($size / 1MB))
        $removed += $size
        continue
    }
    if ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
        # A link is a name, not a tree: remove the name and never follow it.
        $item.Delete()
    } else {
        Remove-Item -LiteralPath $item.FullName -Recurse -Force
    }
    Write-Host ("removed: {0} ({1:N1} MB)" -f $item.Name, ($size / 1MB))
    $removed += $size
}

$after = Get-TreeSize $resolvedTarget
Write-Host ("Target size after: {0:N2} GB" -f ($after / 1GB))
Write-Host ("Freed: {0:N2} GB" -f ($removed / 1GB))
