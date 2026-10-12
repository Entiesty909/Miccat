<#
Slint Live Preview 开发循环：.slint 保存后由运行进程自动重载；Rust 改动重编并重启。

用法：
    .\dev.ps1                 # debug + 进程内 Slint 热更新
    scripts\dev.cmd -Release  # release 构建并监听 Rust 改动，不启用热更新
    scripts\dev.cmd -NoRun    # 只监听并编译
#>
[CmdletBinding()]
param(
    [switch]$Release,
    [switch]$NoRun
)

$ErrorActionPreference = 'Continue'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path.TrimEnd('\')
Set-Location -LiteralPath $root
$buildProfile = if ($Release) { 'release' } else { 'debug' }
$exePath = Join-Path $root "target\$buildProfile\miccat.exe"

# Slint 文件由进程内 Live Preview 监听，不触发 Cargo 重编译。
$watchPaths = @('src', 'build.rs', 'Cargo.toml', 'Cargo.lock') | ForEach-Object { Join-Path $root $_ }
# 产物目录与第三方目录不参与监听。
$excludeDirs = @('target', 'target-ui', '.git', 'node_modules')
$extensions = @('.rs', '.toml', '.lock')
$pollIntervalMs = 500

function Get-WatchStamp {
    # 用轮询而非 FileSystemWatcher：Windows 上 watcher 对频繁写入不可靠。
    $latest = [datetime]::MinValue
    foreach ($watchPath in $watchPaths) {
        if (-not (Test-Path -LiteralPath $watchPath)) { continue }
        $item = Get-Item -LiteralPath $watchPath
        if (-not $item.PSIsContainer) {
            if (($extensions -contains $item.Extension) -and ($item.LastWriteTimeUtc -gt $latest)) {
                $latest = $item.LastWriteTimeUtc
            }
            continue
        }
        Get-ChildItem -LiteralPath $watchPath -Recurse -File |
            Where-Object {
                $relative = $_.FullName.Substring($watchPath.Length).TrimStart('\')
                $topDir = ($relative -split '\\')[0]
                ($excludeDirs -notcontains $topDir) -and ($extensions -contains $_.Extension)
            } |
            ForEach-Object {
                if ($_.LastWriteTimeUtc -gt $latest) { $latest = $_.LastWriteTimeUtc }
            }
    }
    return $latest
}

function Start-Build {
    param([string]$Message)

    Write-Host "[$(Get-Date -Format 'HH:mm:ss')] $Message"
    # 先结束正在运行的程序：否则 cargo 覆盖 exe 时报「拒绝访问 os error 5」。
    Stop-Process -Name 'miccat' -Force -ErrorAction SilentlyContinue
    # 等一会儿，确保文件句柄已经释放。
    Start-Sleep -Milliseconds 300

    $arguments = @('build')
    if ($Release) {
        $arguments += '--release'
        Remove-Item Env:SLINT_LIVE_PREVIEW -ErrorAction SilentlyContinue
    } else {
        $env:SLINT_LIVE_PREVIEW = '1'
        $arguments += @('--features', 'slint/live-preview')
    }
    & (Join-Path $PSScriptRoot 'build-dev.cmd') @arguments
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[$(Get-Date -Format 'HH:mm:ss')] 编译失败，等待下次改动"
        return
    }

    if ($NoRun) {
        Write-Host "[$(Get-Date -Format 'HH:mm:ss')] 编译成功，未启动（-NoRun）"
        return
    }
    if (Test-Path -LiteralPath $exePath) {
        # 不阻塞监听循环。
        Start-Process -FilePath $exePath -WorkingDirectory $root | Out-Null
    }
    Write-Host "[$(Get-Date -Format 'HH:mm:ss')] 编译成功，已启动"
}

if ($Release) {
    Write-Host '模式：Release（禁用 Slint Live Preview）'
} else {
    Write-Host '模式：Debug + Slint Live Preview'
}
if ($NoRun) { Write-Host '模式：-NoRun（只编译，不启动程序）' }
Write-Host '监听目录（递归，.rs / .toml）；.slint 由运行中的 Slint interpreter 自动重载。'
foreach ($watchPath in $watchPaths) { Write-Host "  $watchPath" }
Write-Host '按 Ctrl+C 退出。'

$lastStamp = Get-WatchStamp
if ($lastStamp -eq [datetime]::MinValue) { $lastStamp = (Get-Date).ToUniversalTime() }

Start-Build -Message '首次编译（Slint Live Preview）…'

while ($true) {
    Start-Sleep -Milliseconds $pollIntervalMs
    $stamp = Get-WatchStamp
    if ($stamp -gt $lastStamp) {
        $lastStamp = $stamp
        Start-Build -Message '检测到改动，重新编译…'
    }
}
