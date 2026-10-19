$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path.TrimEnd('\')

# 行数是维护性信号，不是机械拆分的硬目标：常规目标超出时警告，明显失控时才失败。
# 这样允许职责清晰的装配页、桥接契约和 FFI 边界自然超过常规目标，
# 同时保留对单文件无限膨胀的阻断。
$warnings = @()
$violations = @()

foreach ($directory in @('src', 'ui')) {
    $path = Join-Path $root $directory
    Get-ChildItem -LiteralPath $path -Recurse -File |
        Where-Object { $_.Extension -in @('.rs', '.slint') } |
        ForEach-Object {
            $relative = $_.FullName.Substring($root.Length + 1).Replace('\', '/')
            $lineCount = @(Get-Content -LiteralPath $_.FullName).Count
            if ($_.Extension -eq '.slint') {
                $reviewLimit = 450
                $hardLimit = 600
            } else {
                $reviewLimit = 600
                $hardLimit = 800
            }

            if ($lineCount -le $reviewLimit) {
                return
            }
            if ($lineCount -le $hardLimit) {
                $warnings += "$relative ($lineCount lines; review threshold $reviewLimit)"
                return
            }

            $violations += "$relative ($lineCount lines; hard limit $hardLimit)"
        }
}

if ($warnings.Count -gt 0) {
    Write-Warning 'Line-count review warnings:'
    $warnings | ForEach-Object { Write-Warning $_ }
}

if ($violations.Count -gt 0) {
    $violations | ForEach-Object { Write-Error $_ }
    exit 1
}

Write-Host 'Line-count gate passed; review-threshold warnings are non-blocking.'
exit 0

