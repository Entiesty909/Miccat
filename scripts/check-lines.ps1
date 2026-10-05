$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path.TrimEnd('\')

# S0 基线豁免只允许审计时已超限文件不再增长；拆分后删除对应项。
# 若某文件按 AGENTS.md 3.1.2「设计更优」条款合理超限，可抬到新基线并在阶段文档记录原因。
$violations = @()

foreach ($directory in @('src', 'ui')) {
    $path = Join-Path $root $directory
    Get-ChildItem -LiteralPath $path -Recurse -File |
        Where-Object { $_.Extension -in @('.rs', '.slint') } |
        ForEach-Object {
            $relative = $_.FullName.Substring($root.Length + 1).Replace('\', '/')
            $baselineCap = 0
            if ($relative -eq 'src/engine.rs') { $baselineCap = 432 }
            if ($relative -eq 'src/audio/capture.rs') { $baselineCap = 451 }
            # app-window 是纯装配/透传文件：每新增一个跨页回调必然占 2 行
            # （根组件声明 + 向页面转发），无法靠拆分消除。383 → 385 是
            # uninstall-cable 回调的固有成本，见 docs/step-49。
            if ($relative -eq 'ui/app-window.slint') { $baselineCap = 385 }
            $lineCount = @(Get-Content -LiteralPath $_.FullName).Count
            $limit = if ($_.Extension -eq '.slint') { 300 } else { 400 }

            if ($lineCount -le $limit) {
                return
            }

            if ($baselineCap -gt 0 -and $lineCount -le $baselineCap) {
                Write-Host "Baseline over limit: $relative ($lineCount/$limit)"
                return
            }

            $violations += "$relative ($lineCount lines; limit $limit)"
        }
}

if ($violations.Count -gt 0) {
    $violations | ForEach-Object { Write-Error $_ }
    exit 1
}

Write-Host 'Line-count gate passed; legacy files did not grow.'
