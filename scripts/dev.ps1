<#
开发循环：改代码 -> 自动重新编译 -> 自动重启。

用法：
    scripts\dev.cmd                 # debug 构建，编译成功后启动程序
    scripts\dev.cmd -Release        # release 构建
    scripts\dev.cmd -NoRun          # 只监听并编译，不启动程序

说明：Rust 没有真正的热重载，这里用轮询 + 重编 + 重启逼近 npm run dev 的体验。
#>
[CmdletBinding()]
param(
    [switch]$Release,
    [switch]$NoRun
)

$ErrorActionPreference = 'Continue'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path.TrimEnd('\')
$buildProfile = if ($Release) { 'release' } else { 'debug' }
$exePath = Join-Path $root "target\$buildProfile\mintcat-proc-mic.exe"

# 监听目标：仓库根下的这些路径。
$watchPaths = @('src', 'ui', 'build.rs', 'Cargo.toml') | ForEach-Object { Join-Path $root $_ }
# 产物目录与第三方目录不参与监听。
$excludeDirs = @('target', 'target-ui', '.git', 'node_modules')
$extensions = @('.rs', '.slint', '.toml')
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
    Stop-Process -Name 'mintcat-proc-mic' -Force -ErrorAction SilentlyContinue
    # 等一会儿，确保文件句柄已经释放。
    Start-Sleep -Milliseconds 300

    $arguments = @('build')
    if ($Release) { $arguments += '--release' }
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

Write-Host '开发循环：改代码 -> 自动重编 -> 自动重启'
Write-Host "构建配置：$buildProfile"
if ($NoRun) { Write-Host '模式：-NoRun（只编译，不启动程序）' }
Write-Host '监听目录（递归，.rs / .slint / .toml）：'
foreach ($watchPath in $watchPaths) { Write-Host "  $watchPath" }
Write-Host '按 Ctrl+C 退出。'

$lastStamp = Get-WatchStamp
if ($lastStamp -eq [datetime]::MinValue) { $lastStamp = (Get-Date).ToUniversalTime() }

Start-Build -Message '首次编译…'

while ($true) {
    Start-Sleep -Milliseconds $pollIntervalMs
    $stamp = Get-WatchStamp
    if ($stamp -gt $lastStamp) {
        $lastStamp = $stamp
        Start-Build -Message '检测到改动，重新编译…'
    }
}
