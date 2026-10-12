@echo off
setlocal

call "%~dp0build-dev.cmd" build --release
if errorlevel 1 exit /b 1

set "EXE=%~dp0..\target\release\miccat.exe"
for %%I in ("%EXE%") do set "EXE=%%~fI"
if not exist "%EXE%" (
    echo [error] binary not found: %EXE%
    exit /b 1
)

for %%I in ("%EXE%") do set "SIZE=%%~zI"
set "EXPR=[math]::Round(%SIZE% / 1MB, 1).ToString('0.0')"
for /f %%S in ('powershell.exe -NoProfile -Command "%EXPR%"') do set "MB=%%S"

echo %EXE%
echo size: %MB% MB
