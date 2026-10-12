$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'scripts/dev.ps1') @args
exit $LASTEXITCODE
