@echo off
setlocal
cd /d "%~dp0.."
echo [info] workspace: %CD%
call "%~dp0build-dev.cmd" build
if errorlevel 1 (
    echo.
    echo [error] Debug build failed.
    pause
    exit /b 1
)

set "EXE=%~dp0..\target\debug\miccat.exe"
for %%I in ("%EXE%") do set "EXE=%%~fI"
if not exist "%EXE%" (
    echo [error] binary not found: %EXE%
    pause
    exit /b 1
)
echo [ok] Debug binary: %EXE%
pause
exit /b 0
