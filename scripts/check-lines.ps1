$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path.TrimEnd('\')

# S0 基线豁免只允许审计时已超限文件不再增长；拆分后删除对应项。
$violations = @()

foreach ($directory in @('src', 'ui')) {
    $path = Join-Path $root $directory
    Get-ChildItem -LiteralPath $path -Recurse -File |
        Where-Object { $_.Extension -in @('.rs', '.slint') } |
        ForEach-Object {
            $relative = $_.FullName.Substring($root.Length + 1).Replace('\', '/')
            $baselineCap = 0
            if ($relative -eq 'src/ui_bridge.rs') { $baselineCap = 1976 }
            if ($relative -eq 'src/audio/capture.rs') { $baselineCap = 569 }
            if ($relative -eq 'src/engine.rs') { $baselineCap = 551 }
            if ($relative -eq 'ui/app-window.slint') { $baselineCap = 508 }
            if ($relative -eq 'src/config.rs') { $baselineCap = 421 }
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
